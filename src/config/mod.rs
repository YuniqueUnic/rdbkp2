use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock, RwLock};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::{debug, error, info};

static CONFIG: OnceLock<Arc<RwLock<Option<Config>>>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// 备份文件的默认输出目录
    pub backup_dir: PathBuf,

    /// 默认的停止容器执行超时时间，单位为秒
    pub timeout: u64,

    #[serde(rename = "mapper")]
    /// 备份文件的默认输出目录
    pub mapper: BackupMapper,

    /// Docker 相关配置
    pub docker: DockerConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackupMapper {
    pub mapping_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerConfig {
    /// Docker daemon 的地址
    pub host: String,

    /// 是否使用 TLS
    pub tls: bool,

    /// 证书路径 (如果使用 TLS)
    pub cert_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backup_dir: PathBuf::from("./backups"),
            mapper: BackupMapper {
                mapping_path: PathBuf::from("./backups/mapping.toml"),
            },
            docker: DockerConfig {
                host: "unix:///var/run/docker.sock".to_string(),
                tls: false,
                cert_path: None,
            },
            timeout: 30,
        }
    }
}

impl Config {
    /// 获取全局配置实例
    pub fn global() -> Result<Config> {
        let config_lock = CONFIG
            .get()
            .ok_or_else(|| anyhow::anyhow!("Config not initialized"))?;

        let config = config_lock.read().map_err(|e| {
            error!(?e, "Failed to acquire read lock on config");
            anyhow::anyhow!("Failed to read config: {}", e)
        })?;

        Ok(config.clone().unwrap_or_default())
    }

    /// 初始化全局配置
    pub fn init(config: Config) -> Result<()> {
        let res = CONFIG.set(Arc::new(RwLock::new(Some(config))));
        if res.is_err() {
            error!("Failed to set config");
            anyhow::bail!("Failed to set config")
        }
        debug!("Global config initialized");
        Ok(())
    }

    /// 从文件加载配置并初始化全局实例
    pub fn init_from_file<P: AsRef<Path>>(path: P) -> Result<()> {
        let config = Self::load_from_file(path)?;
        Self::init(config)
    }

    /// 从文件加载配置
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            error!(?e, path = ?path.as_ref(), "Failed to read config file");
            e
        })?;
        let config: Config = toml::from_str(&content).map_err(|e| {
            error!(?e, "Failed to parse config file");
            e
        })?;
        debug!(?config, "Config loaded from file");
        Ok(config)
    }

    /// 保存配置到文件，并保留注释
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut content = toml::to_string_pretty(self).map_err(|e| {
            error!(?e, "Failed to serialize config");
            e
        })?;

        // 手动添加注释
        let comments = r#"
    # Docker 容器数据备份工具配置文件

    # 备份文件的默认输出目录
    # backup_dir = "./backups"

    # 停止容器操作的超时时间 (单位：秒)
    # timeout = 30

    # 备份映射文件路径
    # [mapper]
    # mapping_path = "./backups/mapping.toml"

    # Docker 相关配置
    # [docker]
    # Docker daemon 的地址
    # host = "unix:///var/run/docker.sock"
    # 是否使用 TLS
    # tls = false
    # 证书路径 (如果使用 TLS)
    # cert_path = "/path/to/cert"
    "#;

        // 将注释插入到文件内容的前面
        content = format!("{}\n{}", comments.trim(), content);

        std::fs::write(path.as_ref(), content).map_err(|e| {
            error!(?e, path = ?path.as_ref(), "Failed to write config file");
            e
        })?;
        debug!(path = ?path.as_ref(), "Config saved to file");
        Ok(())
    }

    /// 确保备份目录存在
    pub fn ensure_backup_dir(&self) -> Result<()> {
        if !self.backup_dir.exists() {
            std::fs::create_dir_all(&self.backup_dir).map_err(|e| {
                error!(?e, path = ?self.backup_dir, "Failed to create backup directory");
                e
            })?;
            info!(path = ?self.backup_dir, "Backup directory created");
        }
        Ok(())
    }

    /// 更新全局配置
    pub fn update<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Config),
    {
        let config_lock = CONFIG
            .get()
            .ok_or_else(|| anyhow::anyhow!("Config not initialized"))?;

        let mut writer = config_lock.write().map_err(|e| {
            error!(?e, "Failed to acquire write lock on config");
            anyhow::anyhow!("Failed to write config: {}", e)
        })?;

        let mut config = writer.clone().unwrap_or_default();
        f(&mut config);
        *writer = Some(config);

        debug!("Global config updated");
        Ok(())
    }
}

#[allow(dead_code)]

impl BackupMapper {
    pub fn load_mappings(&self) -> Result<HashMap<String, String>> {
        let mappings = mapping::load_mappings(&self.mapping_path)?;
        debug!(?mappings, "Backup mappings loaded");
        Ok(mappings)
    }

    pub fn save_mappings(&self, mappings: &HashMap<String, String>) -> Result<()> {
        mapping::save_mappings(&self.mapping_path, mappings)
    }

    pub fn add_mappings(&self, mappings: impl IntoIterator<Item = (String, String)>) -> Result<()> {
        mapping::add_mappings(&self.mapping_path, mappings)
    }

    pub fn remove_mappings(
        &self,
        keys: impl IntoIterator<Item = String>,
    ) -> Result<impl IntoIterator<Item = (String, String)>> {
        let removed_mappings = mapping::remove_mappings(&self.mapping_path, keys)?;
        let removed_map: HashMap<_, _> = removed_mappings.clone().into_iter().collect();
        self.save_mappings(&removed_map)?;
        Ok(removed_mappings)
    }
}

#[allow(dead_code)]
mod mapping {
    use super::*;

    pub fn load_mappings(backup_mapping_path: &PathBuf) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(backup_mapping_path).map_err(|e| {
            error!(?e, path = ?backup_mapping_path, "Failed to read mapping file");
            e
        })?;
        let mappings: HashMap<String, String> = toml::from_str(&content).map_err(|e| {
            error!(?e, "Failed to parse mapping file");
            e
        })?;
        debug!(?mappings, "Mappings loaded");
        Ok(mappings)
    }

    pub fn save_mappings(
        backup_mapping_path: &PathBuf,
        mappings: &HashMap<String, String>,
    ) -> Result<()> {
        let content = toml::to_string_pretty(mappings).map_err(|e| {
            error!(?e, "Failed to serialize mappings");
            e
        })?;
        std::fs::write(backup_mapping_path, content).map_err(|e| {
            error!(?e, path = ?backup_mapping_path, "Failed to write mapping file");
            e
        })?;
        debug!(path = ?backup_mapping_path, "Mappings saved");
        Ok(())
    }

    pub fn add_mappings(
        backup_mapping_path: &PathBuf,
        mapping: impl IntoIterator<Item = (String, String)>,
    ) -> Result<()> {
        let mut existing_mapping = load_mappings(backup_mapping_path)?;
        for (key, value) in mapping {
            existing_mapping.insert(key.clone(), value.clone());
            debug!(key = ?key, value = ?value, "Added mapping");
        }
        save_mappings(backup_mapping_path, &existing_mapping)
    }

    pub fn remove_mappings(
        backup_mapping_path: &PathBuf,
        keys: impl IntoIterator<Item = String>,
    ) -> Result<Vec<(String, String)>> {
        let mut existing_mapping = load_mappings(backup_mapping_path)?;
        let mut removed_mappings = Vec::new();
        for key in keys {
            if let Some(value) = existing_mapping.remove(&key) {
                removed_mappings.push((key.clone(), value.clone()));
                debug!(key = ?key, value = ?value, "Removed mapping");
            }
        }
        save_mappings(backup_mapping_path, &existing_mapping)?;
        Ok(removed_mappings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn test_config_singleton() -> Result<()> {
        // 创建测试配置
        let test_config = Config::default();

        // 初始化全局配置
        Config::init(test_config.clone())?;

        // 获取全局配置并验证
        let global_config = Config::global()?;
        assert_eq!(global_config.backup_dir, PathBuf::from("./backups"));

        // 测试更新配置
        println!("Updating config");
        Config::global()?.update(|config| {
            config.backup_dir = PathBuf::from("./new_backups");
        })?;

        // 验证更新后的配置
        let updated_config = Config::global()?;
        assert_eq!(updated_config.backup_dir, PathBuf::from("./new_backups"));

        Ok(())
    }

    #[test]
    fn test_config_file_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("config.toml");

        // 创建并保存配置
        let config = Config::default();
        config.save_to_file(&config_path)?;

        // 从文件加载配置
        let loaded_config = Config::load_from_file(&config_path)?;
        assert_eq!(loaded_config.backup_dir, PathBuf::from("./backups"));

        Ok(())
    }
}
