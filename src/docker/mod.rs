use anyhow::{Context, Result};
use bollard::{
    Docker,
    container::{InspectContainerOptions, ListContainersOptions},
    secret::ContainerStateStatusEnum,
};
use mockall::{automock, predicate::*};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};
use tracing::{debug, error, info, warn};

use crate::utils;

// 定义 DockerClient 接口 trait，并使用 automock 为 test 生成 mock 实现
#[automock]
#[allow(dead_code)]
pub trait DockerClientInterface: Send + Sync + Clone + 'static {
    async fn list_containers(&self) -> Result<Vec<ContainerInfo>>;
    async fn get_container_volumes(&self, container_id: &str) -> Result<Vec<VolumeInfo>>;
    async fn start_container(&self, container_id: &str) -> Result<()>;
    async fn restart_container(&self, container_id: &str) -> Result<()>;
    async fn stop_container(&self, container_id: &str) -> Result<()>;
    async fn get_container_working_dir(&self, id: &str) -> Result<String>;
    async fn get_container_status(&self, id: &str) -> Result<String>;
    fn get_stop_timeout_secs(&self) -> u64;
}

impl Clone for MockDockerClientInterface {
    fn clone(&self) -> Self {
        let mut client = MockDockerClientInterface::new();
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));
        client.expect_stop_container().returning(|_| Ok(()));
        client.expect_get_stop_timeout_secs().returning(|| 10);
        client.expect_restart_container().returning(|_| Ok(()));
        client
    }
}

// 使用别名，方便在 #[cfg(test)] 环境下替换为 Mock 类型
#[cfg(not(test))]
pub type ClientType = DockerClient;

#[cfg(test)]
pub type ClientType = MockDockerClientInterface; //  test 环境下 MockableDockerClient  是 MockDockerClientInterface

pub(crate) static DOCKER_CLIENT_INSTANCE: OnceLock<Arc<RwLock<ClientType>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct DockerClient {
    client: Docker,
    stop_timeout_secs: u64,
}

impl DockerClient {
    pub fn global() -> Result<ClientType> {
        let client_arc_lock = DOCKER_CLIENT_INSTANCE
            .get()
            .ok_or_else(|| anyhow::anyhow!("Docker client not initialized"))?;

        let client_read_guard = client_arc_lock
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock on Docker client: {}", e))?;

        Ok(client_read_guard.clone())
    }

    /// Initialize the global Docker client instance
    #[cfg(not(test))]
    pub fn init(stop_timeout_secs: u64) -> Result<()> {
        let client = DockerClient::new(stop_timeout_secs)?;
        let arc = Arc::new(RwLock::new(client));
        DOCKER_CLIENT_INSTANCE.get_or_init(|| arc);
        Ok(())
    }

    /// Initialize a mock Docker client for testing
    #[cfg(test)]
    pub fn init(_stop_timeout_secs: u64) -> Result<()> {
        let client = MockDockerClientInterface::new();
        let arc = Arc::new(RwLock::new(client));
        DOCKER_CLIENT_INSTANCE.get_or_init(|| arc);
        Ok(())
    }

    /// 创建新的 Docker 客户端
    #[allow(dead_code)]
    fn new(stop_timeout_secs: u64) -> Result<Self> {
        debug!("Initializing Docker client");
        let client = Docker::connect_with_local_defaults().map_err(|e| {
            error!(?e, "Failed to connect to Docker daemon");
            e
        })?;
        info!("Docker client initialized successfully");
        Ok(Self {
            client,
            stop_timeout_secs,
        })
    }
}

impl DockerClientInterface for DockerClient {
    /// 列出所有容器
    async fn list_containers(&self) -> Result<Vec<ContainerInfo>> {
        debug!("Listing all containers");
        let options = Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await.map_err(|e| {
            error!(?e, "Failed to list containers");
            e
        })?;

        let mut result = Vec::new();
        for container in containers {
            let name = container
                .names
                .unwrap_or_default()
                .first()
                .cloned()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();

            debug!(container_id = ?container.id, container_name = ?name, "Found container");
            result.push(ContainerInfo {
                id: container.id.unwrap_or_default(),
                name,
                status: container.status.unwrap_or_default(),
            });
        }

        info!(
            container_count = result.len(),
            "Successfully listed containers"
        );
        Ok(result)
    }

    /// 获取容器的卷信息
    async fn get_container_volumes(&self, container_id: &str) -> Result<Vec<VolumeInfo>> {
        debug!(container_id, "Getting volume information");
        let details = self
            .client
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| {
                error!(?e, container_id, "Failed to inspect container");
                e
            })?;

        let working_dir = self.get_container_working_dir(container_id).await?;
        let working_dir_path = PathBuf::from(&working_dir);
        let mounts = details.mounts.unwrap_or_default();
        let mut volumes = Vec::new();

        for mount in mounts {
            if let (Some(source), Some(destination)) =
                (mount.source.clone(), mount.destination.clone())
            {
                debug!(
                    source = ?source,
                    destination = ?destination,
                    "Found volume mount"
                );

                let source = PathBuf::from(source);
                let destination = PathBuf::from(destination);

                // 将 source 转换为绝对路径
                // 存在性检查
                let source = utils::absolute_canonicalize_path(&source)
                    .context("Failed to canonicalize path for volume mount source")?;

                // 将 destination 转化为容器内部的路径
                // 容器内部路径，则不应该检查路径是否存在
                // 而是应该获取容器的内部 WorkingDir 作为基本路径
                let destination = utils::ensure_absolute_canonical(&destination, &working_dir_path)
                    .context("Failed to canonicalize path for volume mount destination")?;

                volumes.push(VolumeInfo {
                    source,
                    destination,
                    name: mount.name.unwrap_or_default(),
                });
            } else {
                warn!(
                    mount_type = ?mount.typ,
                    "Skipping mount point due to missing source or destination"
                );
            }
        }

        if volumes.is_empty() {
            warn!(container_id, "No volumes found for container");
        } else {
            info!(
                container_id,
                volume_count = volumes.len(),
                "Successfully retrieved volume information"
            );
        }

        Ok(volumes)
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        debug!("Starting container: {}", container_id);

        self.client
            .start_container::<String>(container_id, None)
            .await
            .map_err(|e| {
                error!(?e, "Failed to start container");
                e
            })?;

        debug!("Container started: {:?}", container_id);

        Ok(())
    }

    async fn restart_container(&self, container_id: &str) -> Result<()> {
        debug!("Restarting container: {}", container_id);

        self.client
            .restart_container(container_id, None)
            .await
            .map_err(|e| {
                error!(?e, "Failed to restart container");
                e
            })?;

        debug!("Container restarted: {:?}", container_id);

        Ok(())
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        debug!("Stopping container: {}", container_id);

        self.client
            .stop_container(container_id, None)
            .await
            .map_err(|e| {
                error!(?e, "Failed to stop container");
                e
            })?;

        debug!("Container stopped: {:?}", container_id);

        Ok(())
    }

    async fn get_container_status(&self, id: &str) -> Result<String> {
        let status = self
            .client
            .inspect_container(id, None::<InspectContainerOptions>)
            .await?;
        match_status(status)
    }

    fn get_stop_timeout_secs(&self) -> u64 {
        self.stop_timeout_secs
    }

    async fn get_container_working_dir(&self, id: &str) -> Result<String> {
        let status = self
            .client
            .inspect_container(id, None::<InspectContainerOptions>)
            .await?;
        let config = status
            .config
            .ok_or_else(|| anyhow::anyhow!("container config not found"))?;

        let working_dir = config
            .working_dir
            .ok_or_else(|| anyhow::anyhow!("container working dir not found"))?;

        Ok(working_dir)
    }
}

/// 匹配容器状态
///
/// 将 bollard::secret::ContainerInspectResponse 中的状态转换为字符串
fn match_status(status: bollard::secret::ContainerInspectResponse) -> Result<String> {
    match status.state {
        Some(state) => match state.status {
            Some(ContainerStateStatusEnum::RUNNING) => Ok("running".to_string()),
            Some(ContainerStateStatusEnum::PAUSED) => Ok("paused".to_string()),
            Some(ContainerStateStatusEnum::RESTARTING) => Ok("restarting".to_string()),
            Some(ContainerStateStatusEnum::EXITED) => Ok("exited".to_string()),
            Some(ContainerStateStatusEnum::DEAD) => Ok("dead".to_string()),
            _ => Err(anyhow::anyhow!("Container status not found")),
        },
        None => Err(anyhow::anyhow!("Container status not found")),
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackupMapping {
    /// 容器名称
    pub container_name: String,
    /// 容器 ID
    pub container_id: String,
    /// 备份的卷信息
    pub volumes: Vec<VolumeInfo>,
    /// 备份时间
    pub backup_time: String,
    /// 备份版本
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub name: String,
    pub source: PathBuf,
    pub destination: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DOCKER_COMPOSE_CMD,
        tests::{check_docker_compose, get_docker_compose_path},
    };
    use std::sync::Once;
    use std::time::Duration;
    use tokio::{self, process::Command, time::sleep};
    use tracing::debug;
    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            // Initialize logging for tests if needed
            // 初始化日志
            crate::tests::init_test_log();
            // 初始化 DockerClient
            DockerClient::init(10).unwrap();
        });
    }

    #[tokio::test]
    async fn test_docker_client_creation() -> Result<()> {
        setup();
        let _client = DockerClient::global()?;
        Ok(())
    }

    #[tokio::test]
    async fn test_list_containers() {
        setup();
        // Initialize the global mock client
        DockerClient::init(0).unwrap();

        // Get the client and call the method
        let mut client = DockerClient::global().unwrap();
        // Set expectations
        client.expect_list_containers().times(1).returning(|| {
            Ok(vec![ContainerInfo {
                id: "container1".to_string(),
                name: "test-container".to_string(),
                status: "running".to_string(),
            }])
        });

        let containers = client.list_containers().await.unwrap();

        // Verify results
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].id, "container1");
        assert_eq!(containers[0].name, "test-container");
        assert_eq!(containers[0].status, "running");
    }

    #[tokio::test]
    async fn test_get_container_volumes_simple() {
        setup();
        // Initialize the global mock client
        DockerClient::init(0).unwrap();

        // Get the client and call the method
        let mut client = DockerClient::global().unwrap();
        // Set expectations
        client
            .expect_get_container_volumes()
            .with(eq("container1"))
            .times(1)
            .returning(|_| {
                Ok(vec![VolumeInfo {
                    name: "volume1".to_string(),
                    source: PathBuf::from("/host/path"),
                    destination: PathBuf::from("/container/path"),
                }])
            });

        let volumes = client.get_container_volumes("container1").await.unwrap();

        // Verify results
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "volume1");
        assert_eq!(volumes[0].source, PathBuf::from("/host/path"));
        assert_eq!(volumes[0].destination, PathBuf::from("/container/path"));
    }

    #[tokio::test]
    async fn test_list_containers_simple() -> Result<()> {
        setup();
        let mut client = DockerClient::global()?;
        client.expect_list_containers().returning(|| {
            Ok(vec![ContainerInfo {
                id: "test_id_1".to_string(),
                name: "test_container_1".to_string(),
                status: "running".to_string(),
            }])
        });
        let containers = client.list_containers().await?;
        assert!(containers.len() == 1);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "This test needs a really docker environment, manual test is recommended"]
    async fn test_get_container_volumes() -> Result<()> {
        setup();

        // 首先检查 docker compose 命令是否可用
        check_docker_compose()?;

        let client = DockerClient::global()?;
        let docker_dir = get_docker_compose_path();

        debug!("Docker directory: {:?}", docker_dir);
        assert!(docker_dir.exists(), "Docker directory does not exist");
        assert!(
            docker_dir.join("docker-compose.yaml").exists(),
            "docker-compose.yaml not found"
        );

        let docker_compose_file = docker_dir.join("docker-compose.yaml");
        assert!(
            docker_compose_file.exists(),
            "docker-compose.yaml not found"
        );

        // 先确保清理旧的容器
        debug!("Cleaning up old containers...");
        let cleanup = Command::new(DOCKER_COMPOSE_CMD)
            .current_dir(&docker_dir)
            .args(["-f", &docker_compose_file.to_string_lossy(), "down", "-v"])
            .output()
            .await?;

        if !cleanup.status.success() {
            debug!(
                "Cleanup stderr: {}",
                String::from_utf8_lossy(&cleanup.stderr)
            );
            return Err(anyhow::anyhow!("Failed to cleanup old containers"));
        }

        // 确保测试容器运行中
        debug!("Starting containers...");
        let output = Command::new(DOCKER_COMPOSE_CMD)
            .current_dir(&docker_dir)
            .args(["-f", &docker_compose_file.to_string_lossy(), "up", "-d"])
            .output()
            .await?;

        if !output.status.success() {
            debug!(
                "Docker compose stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            debug!(
                "Docker compose stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            return Err(anyhow::anyhow!("Failed to start docker container"));
        }

        debug!("Waiting for containers to start...");
        sleep(Duration::from_secs(5)).await;

        let containers = client.list_containers().await?;
        debug!("Found containers: {:?}", containers);

        let sim_server = containers
            .iter()
            .find(|c| c.name == "sim-server")
            .ok_or_else(|| anyhow::anyhow!("sim-server container not found"))?;

        debug!("Found sim-server container: {:?}", sim_server);
        let volumes = client.get_container_volumes(&sim_server.id).await?;
        debug!("Found volumes: {:?}", volumes);
        assert!(!volumes.is_empty());

        // 清理
        debug!("Cleaning up test environment...");
        let cleanup = Command::new(DOCKER_COMPOSE_CMD)
            .current_dir(&docker_dir)
            .args(["-f", &docker_compose_file.to_string_lossy(), "down", "-v"])
            .output()
            .await?;

        if !cleanup.status.success() {
            debug!(
                "Final cleanup stderr: {}",
                String::from_utf8_lossy(&cleanup.stderr)
            );
            return Err(anyhow::anyhow!("Failed to cleanup test environment"));
        }

        Ok(())
    }
}
