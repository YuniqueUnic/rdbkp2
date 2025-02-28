use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// 备份文件的默认输出目录
    pub backup_dir: PathBuf,

    /// 备份文件的默认输出目录
    pub backup_mapper: BackupMapper,

    /// Docker 相关配置
    pub docker: DockerConfig,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMapper {
    pub backup_mapping_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
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
            backup_mapper: BackupMapper {
                backup_mapping_path: PathBuf::from("./backup_mapping.toml"),
            },
            docker: DockerConfig {
                host: "unix:///var/run/docker.sock".to_string(),
                tls: false,
                cert_path: None,
            },
        }
    }
}

#[allow(dead_code)]
impl Config {
    /// 从文件加载配置
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// 保存配置到文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 确保备份目录存在
    pub fn ensure_backup_dir(&self) -> Result<()> {
        if !self.backup_dir.exists() {
            std::fs::create_dir_all(&self.backup_dir)?;
        }
        Ok(())
    }
}

impl BackupMapper {
    pub fn load_mappings(&self) -> Result<HashMap<String, String>> {
        let mappings = mapping::load_mappings(&self.backup_mapping_path)?;
        Ok(mappings)
    }

    pub fn save_mappings(&self, mappings: &HashMap<String, String>) -> Result<()> {
        mapping::save_mappings(&self.backup_mapping_path, mappings)
    }

    pub fn add_mappings(&self, mappings: impl IntoIterator<Item = (String, String)>) -> Result<()> {
        mapping::add_mappings(&self.backup_mapping_path, mappings)
    }

    pub fn remove_mappings(
        &self,
        keys: impl IntoIterator<Item = String>,
    ) -> Result<impl IntoIterator<Item = (String, String)>> {
        let removed_mappings = mapping::remove_mappings(&self.backup_mapping_path, keys)?;
        let removed_map: HashMap<_, _> = removed_mappings.clone().into_iter().collect();
        self.save_mappings(&removed_map)?;
        Ok(removed_mappings)
    }
}

mod mapping {
    use std::{collections::HashMap, path::PathBuf};

    use anyhow::Result;

    pub fn load_mappings(backup_mapping_path: &PathBuf) -> Result<HashMap<String, String>> {
        let content = std::fs::read_to_string(backup_mapping_path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save_mappings(
        backup_mapping_path: &PathBuf,
        mappings: &HashMap<String, String>,
    ) -> Result<()> {
        let content = toml::to_string_pretty(mappings)?;
        std::fs::write(backup_mapping_path.clone(), content)?;
        Ok(())
    }

    pub fn add_mappings(
        backup_mapping_path: &PathBuf,
        mapping: impl IntoIterator<Item = (String, String)>,
    ) -> Result<()> {
        let mut existing_mapping = load_mappings(backup_mapping_path)?;
        for (key, value) in mapping {
            existing_mapping.insert(key, value);
        }
        save_mappings(backup_mapping_path, &existing_mapping)?;
        Ok(())
    }

    pub fn remove_mappings(
        backup_mapping_path: &PathBuf,
        keys: impl IntoIterator<Item = String>,
    ) -> Result<Vec<(String, String)>> {
        let mut existing_mapping = load_mappings(backup_mapping_path)?;
        let mut removed_mappings = Vec::new();
        for key in keys {
            let value = existing_mapping.remove(&key);
            if let Some(value) = value {
                removed_mappings.push((key, value));
            }
        }
        save_mappings(backup_mapping_path, &existing_mapping)?;
        Ok(removed_mappings)
    }
}
