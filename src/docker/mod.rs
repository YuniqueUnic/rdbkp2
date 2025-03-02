use anyhow::Result;
use bollard::Docker;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::secret::ContainerStateStatusEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use tracing::{debug, error, info, warn};

pub struct DockerClient {
    client: Docker,
}

impl DockerClient {
    /// 创建新的 Docker 客户端
    pub async fn new() -> Result<Self> {
        debug!("Initializing Docker client");
        let client = Docker::connect_with_local_defaults().map_err(|e| {
            error!(?e, "Failed to connect to Docker daemon");
            e
        })?;
        info!("Docker client initialized successfully");
        Ok(Self { client })
    }

    /// 列出所有容器
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>> {
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
    pub async fn get_container_volumes(&self, container_id: &str) -> Result<Vec<VolumeInfo>> {
        debug!(container_id, "Getting volume information");
        let details = self
            .client
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| {
                error!(?e, container_id, "Failed to inspect container");
                e
            })?;

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
                volumes.push(VolumeInfo {
                    source: PathBuf::from(source),
                    destination: PathBuf::from(destination),
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

    #[allow(dead_code)]
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
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

    pub async fn restart_container(&self, container_id: &str) -> Result<()> {
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

    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
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

    pub(crate) async fn get_container_status(&self, id: &str) -> Result<String> {
        let status = self
            .client
            .inspect_container(id, None::<InspectContainerOptions>)
            .await?;
        match_status(status)
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
    use std::time::Duration;
    use tokio::{self, process::Command, time::sleep};
    use tracing::debug;

    #[tokio::test]
    async fn test_docker_client_creation() -> Result<()> {
        let _client = DockerClient::new().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_list_containers() -> Result<()> {
        let client = DockerClient::new().await?;
        let containers = client.list_containers().await?;
        assert!(containers.len() >= 0);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_container_volumes() -> Result<()> {
        crate::init_test_log();

        // 首先检查 docker compose 命令是否可用
        check_docker_compose()?;

        let client = DockerClient::new().await?;
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
