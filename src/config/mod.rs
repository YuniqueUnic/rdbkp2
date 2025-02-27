use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// 备份文件的默认输出目录
    pub backup_dir: PathBuf,

    /// Docker 相关配置
    pub docker: DockerConfig,
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
