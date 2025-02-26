use anyhow::Result;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::Docker;
use std::collections::HashMap;
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
                    mount_type = ?mount.type_,
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
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub name: String,
}
