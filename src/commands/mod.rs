use crate::config::Config;
use crate::docker::{ContainerInfo, DockerClient, VolumeInfo};
use crate::utils::{
    compress_directory, create_timestamp_filename, ensure_dir_exists, extract_archive,
};
use anyhow::Result;
use dialoguer::{Confirm, Input, Select};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub async fn list_containers() -> Result<()> {
    debug!("Initializing Docker client for container listing");
    let client = DockerClient::new().await?;

    debug!("Retrieving container list");
    let containers = client.list_containers().await?;

    println!("\nAvailable containers:");
    println!("{:<20} {:<40} {:<20}", "NAME", "ID", "STATUS");
    println!("{:-<80}", "");

    for container in &containers {
        println!(
            "{:<20} {:<40} {:<20}",
            container.name, container.id, container.status
        );
    }

    info!(
        container_count = containers.len(),
        "Container list displayed"
    );
    Ok(())
}

async fn stop_container_timeout(
    client: &DockerClient,
    container_info: &ContainerInfo,
    timeout: u64,
) -> Result<()> {
    // 首先尝试停止容器
    debug!("Attempting to stop container {}", container_info.id);
    client.stop_container(&container_info.id).await?;

    // 然后等待容器完全停止
    let timer = tokio::time::timeout(Duration::from_secs(timeout), async move {
        loop {
            match client.get_container_status(&container_info.id).await {
                Ok(status) => {
                    if status != "running" && status != "restarting" {
                        debug!("Container stopped successfully with status: {}", status);
                        return Ok(());
                    }
                }
                Err(e) => {
                    error!("Failed to get container status: {}", e);
                    return Err(e);
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // 处理超时情况
    match timer.await {
        Ok(result) => result.map_err(|e| anyhow::anyhow!("Failed to stop container: {}", e)),
        Err(_) => {
            error!("Timeout while waiting for container to stop");
            Err(anyhow::anyhow!(
                "Timeout while waiting for container to stop"
            ))
        }
    }
}

pub async fn backup(
    container: Option<String>,
    data_path: Option<String>,
    output: Option<String>,
    restart: bool,
    interactive: bool,
    timeout: u64,
) -> Result<()> {
    info!(
        ?container,
        ?data_path,
        ?output,
        restart,
        interactive,
        "Starting backup operation"
    );

    let client = DockerClient::new().await?;
    let config = Config::default();

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        select_container(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取输出目录
    debug!(container_name = ?container_info.name, "Getting output directory");
    let output_dir = if interactive || output.is_none() {
        let default_dir = config.backup_dir.to_string_lossy();
        let input: String = Input::new()
            .with_prompt("Backup output directory")
            .default(default_dir.to_string())
            .interact_text()?;
        PathBuf::from(input)
    } else {
        match output {
            Some(output) => PathBuf::from(output),
            None => {
                error!("Output directory is required");
                println!("Output directory is required");
                return Ok(());
            }
        }
    };

    // 确保输出目录存在
    debug!(?output_dir, "Ensuring output directory exists");
    ensure_dir_exists(&output_dir)?;

    // 如果容器正在运行或重启中，则停止容器
    stop_container_timeout(&client, &container_info, timeout).await?;

    // 如果设置了数据路径，则将只备份该路径下的数据
    #[allow(unused_assignments)]
    let mut selected_volumes = Vec::new();
    if let Some(data_path) = data_path {
        let data_path = PathBuf::from(data_path);
        if !data_path.exists() {
            error!("Data path does not exist: {}", data_path.display());
            println!("Data path does not exist: {}", data_path.display());
            return Ok(());
        }

        // 获取卷信息
        selected_volumes = vec![VolumeInfo {
            source: data_path.clone(),
            destination: data_path.clone(),
            name: data_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        }];
    } else {
        // 获取卷信息
        debug!(container_id = ?container_info.id, "Getting volume information");
        let volumes = client.get_container_volumes(&container_info.id).await?;
        if volumes.is_empty() {
            warn!(container_name = ?container_info.name, "No volumes found for container");
            println!("No volumes found for container {}", container_info.name);
            return Ok(());
        }

        // 选择要备份的卷
        debug!(volume_count = volumes.len(), "Selecting volumes to backup");
        selected_volumes = if interactive {
            select_volumes(&volumes)?
        } else {
            volumes
        };
    }

    // 执行备份
    for volume in selected_volumes {
        backup_volume(&container_info, &volume, &output_dir).await?;
    }

    info!(
        container_name = ?container_info.name,
        "Backup operation completed successfully"
    );

    if restart {
        info!(
            container_name = ?container_info.name,
            "Restarting container"
        );
        client.restart_container(&container_info.id).await?;
        info!(
            container_name = ?container_info.name,
            "Container restarted"
        );
    }

    Ok(())
}

pub async fn restore(
    container: Option<String>,
    input: Option<String>,
    restart: bool,
    interactive: bool,
    timeout: u64,
) -> Result<()> {
    info!(
        ?container,
        ?input,
        restart,
        interactive,
        "Starting restore operation"
    );

    let client = DockerClient::new().await?;

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        select_container(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取备份文件路径
    debug!(container_name = ?container_info.name, "Getting backup file/dir path");
    let backup_path = if interactive || input.is_none() {
        let input: String = Input::new()
            .with_prompt("Backup file/dir path")
            .interact_text()?;
        PathBuf::from(input)
    } else {
        match input {
            Some(input) => PathBuf::from(input),
            None => {
                error!("Backup file/dir path is required");
                println!("Backup file/dir path is required");
                return Ok(());
            }
        }
    };

    // 确保备份文件/目录存在
    debug!(backup_path = ?backup_path, "Ensuring backup file/dir exists");
    if !backup_path.exists() {
        error!("Backup file/dir does not exist: {}", backup_path.display());
        println!("Backup file/dir does not exist: {}", backup_path.display());
        return Ok(());
    }

    // 如果容器正在运行或重启中，则停止容器
    stop_container_timeout(&client, &container_info, timeout).await?;

    // 确认恢复
    if interactive {
        debug!("Requesting user confirmation");
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to restore {} to container {}?",
                backup_path.display(),
                container_info.name
            ))
            .interact()?;

        if !confirmed {
            info!("Restore operation cancelled by user");
            println!("Restore cancelled");
            return Ok(());
        }
    }

    // 执行恢复
    restore_volume(&container_info, &backup_path).await?;

    info!(
        container_name = ?container_info.name,
        "Restore operation completed successfully"
    );

    if restart {
        info!(
            container_name = ?container_info.name,
            "Restarting container"
        );
        client.restart_container(&container_info.id).await?;
        info!(
            container_name = ?container_info.name,
            "Container restarted"
        );
    }

    Ok(())
}

async fn select_container(client: &DockerClient) -> Result<ContainerInfo> {
    debug!("Getting container list for selection");
    let containers = client.list_containers().await?;
    let container_names: Vec<&String> = containers.iter().map(|c| &c.name).collect();

    debug!("Displaying container selection prompt");
    let selection = Select::new()
        .with_prompt("Select container")
        .items(&container_names)
        .interact()?;

    let selected = containers[selection].clone();
    info!(
        container_name = ?selected.name,
        container_id = ?selected.id,
        "Container selected"
    );
    Ok(selected)
}

fn select_volumes(volumes: &[VolumeInfo]) -> Result<Vec<VolumeInfo>> {
    debug!(volume_count = volumes.len(), "Preparing volume selection");
    let volume_names: Vec<String> = volumes
        .iter()
        .map(|v| format!("{} -> {}", v.source.display(), v.destination.display()))
        .collect();

    debug!("Displaying volume selection prompt");
    let selections = Select::new()
        .with_prompt("[use arrow keys to move, press enter to select]\r\nSelect volumes to backup")
        .items(&volume_names)
        .interact()?;

    let selected = vec![volumes[selections].clone()];
    info!(
        selected_volume = ?selected[0].name,
        "Volume selected"
    );
    Ok(selected)
}

async fn get_container_by_name_or_id(
    client: &DockerClient,
    name_or_id: &str,
) -> Result<ContainerInfo> {
    debug!(?name_or_id, "Looking up container by name or ID");
    let containers = client.list_containers().await?;
    containers
        .into_iter()
        .find(|c| c.name == name_or_id || c.id == name_or_id)
        .ok_or_else(|| {
            let err = anyhow::anyhow!("Container not found: {}", name_or_id);
            error!(?name_or_id, "Container not found");
            err
        })
}

async fn backup_volume(
    container: &ContainerInfo,
    volume: &VolumeInfo,
    output_dir: &Path,
) -> Result<()> {
    info!(
        container_name = ?container.name,
        volume_name = ?volume.name,
        "Starting volume backup"
    );

    println!(
        "Backing up volume {} from container {} to {}",
        volume.name,
        container.name,
        output_dir.display()
    );

    // 创建备份文件名
    debug!("Creating backup filename");
    let backup_filename =
        create_timestamp_filename(&format!("{}_{}", container.name, volume.name), ".tar.xz");
    let backup_path = output_dir.join(backup_filename);

    debug!(
        source = ?volume.source,
        destination = ?backup_path,
        "Compressing volume directory"
    );

    // 压缩卷目录
    compress_directory(
        &volume.source,
        &backup_path,
        &[".git", "node_modules", "target"],
    )?;

    info!(
        backup_file = ?backup_path,
        "Volume backup completed successfully"
    );
    println!("Backup completed: {}", backup_path.display());
    Ok(())
}

async fn restore_volume(container: &ContainerInfo, backup_path: &PathBuf) -> Result<()> {
    info!(
        container_name = ?container.name,
        backup_path = ?backup_path,
        "Starting volume restore"
    );

    println!(
        "Restoring {} to container {}",
        backup_path.display(),
        container.name
    );

    // 获取卷信息
    debug!(container_id = ?container.id, "Getting volume information");
    let client = DockerClient::new().await?;
    let volumes = client.get_container_volumes(&container.id).await?;

    // 选择要恢复的卷
    debug!(volume_count = volumes.len(), "Selecting target volume");
    let volume = if volumes.len() == 1 {
        volumes[0].clone()
    } else {
        select_volumes(&volumes)?[0].clone()
    };

    debug!(
        source = ?backup_path,
        destination = ?volume.source,
        "Extracting backup archive"
    );

    // 解压备份文件到卷目录
    extract_archive(backup_path, &volume.source)?;

    info!(
        volume_name = ?volume.name,
        "Volume restore completed successfully"
    );
    println!("Restore completed to {}", volume.source.display());
    Ok(())
}
