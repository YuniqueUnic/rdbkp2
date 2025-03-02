mod prompt;

use crate::config::Config;
use crate::docker::{BackupMapping, ContainerInfo, DockerClient, VolumeInfo};
use crate::utils::{self, create_timestamp_filename, ensure_dir_exists, extract_archive};
use prompt::*;

use anyhow::Result;
use chrono::Local;
use dialoguer::{Confirm, Input, Select};
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml;
use tracing::{debug, error, info, warn};

#[macro_export]
macro_rules! prompt_select {
    ($prompt_str:expr) => {
        format!(
            "[💡 use arrow keys to move, press [enter] to select]\r\n{}",
            $prompt_str
        )
    };
}

const MAPPING_FILE_NAME: &str = "mapping.toml";

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
    timeout_secs: u64, // 修改为 timeout_secs，更清晰地表明单位是秒
) -> Result<()> {
    // 首先尝试停止容器
    println!("Attempting to stop container {}", container_info.id);
    debug!("Attempting to stop container {}", container_info.id);
    client.stop_container(&container_info.id).await?;

    // 然后等待容器完全停止，并添加终端输出反馈
    let timer_result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            match client.get_container_status(&container_info.id).await {
                Ok(status) => {
                    if status != "running" && status != "restarting" {
                        info!(
                            "Container {} stopped successfully with status: {}",
                            container_info.id, status
                        );
                        return Ok(()); // 容器成功停止，返回 Ok
                    } else {
                        info!(
                            "Container {} still stopping, current status: {}",
                            container_info.id, status
                        ); // 输出反馈信息
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to get container status for container {}: {}",
                        container_info.id, e
                    );
                    return Err(anyhow::anyhow!("Failed to get container status: {}", e)); // 获取状态失败，返回 Err
                }
            };
            tokio::time::sleep(Duration::from_secs(1)).await; // 每秒检查一次状态
        }
    })
    .await;

    // 处理超时情况和结果
    match timer_result {
        Ok(result) => result
            .map_err(|e| anyhow::anyhow!("Failed to stop container {}: {}", container_info.id, e)),
        Err(_timeout_err) => {
            // _timeout_err 是 tokio::time::error::Elapsed 类型的错误
            error!(
                "Timeout while waiting for container {} to stop after {} seconds",
                container_info.id, timeout_secs
            );
            Err(anyhow::anyhow!(
                "Timeout while waiting for container {} to stop after {} seconds",
                container_info.id,
                timeout_secs
            ))
        }
    }
}

pub async fn backup(
    container: Option<String>,
    file: Option<String>,
    output: Option<String>,
    restart: bool,
    interactive: bool,
    timeout: u64,
    exclude_patterns: &[&str],
) -> Result<()> {
    info!(
        ?container,
        ?file,
        ?output,
        restart,
        interactive,
        "Starting backup operation"
    );

    let client = DockerClient::new().await?;

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        prompt::select_container_prompt(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取输出目录
    let output_dir = parse_output_dir(output, interactive, &container_info)?;

    // 如果容器正在运行或重启中，则停止容器
    stop_container_timeout(&client, &container_info, timeout).await?;

    // 如果设置了数据路径，则将只备份该路径下的数据
    let (total_volumes, selected_volumes) =
        select_volumes(file, interactive, &client, &container_info).await?;

    // 排除模式 (从命令行参数获取，默认值为 ".git,node_modules,target")
    // let exclude_patterns = &[".git", "node_modules", "target"];

    // 备份卷 (s)
    backup_items(
        &container_info,
        output_dir,
        total_volumes,
        selected_volumes,
        exclude_patterns,
    )?;

    // 如果需要重启容器，则重启容器
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

fn parse_output_dir(
    output: Option<String>,
    interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    let config = Config::global()?;

    // 获取输出目录
    debug!(container_name = ?container_info.name, "Getting output directory");
    let output_dir = if interactive || output.is_none() {
        let default_dir = if let Some(output) = output {
            output
        } else {
            config.backup_dir.to_string_lossy().to_string()
        };

        let input: String = Input::new()
            .with_prompt("Backup output directory")
            .default(default_dir)
            .allow_empty(false)
            .interact_text()?;

        PathBuf::from(input)
    } else {
        match output {
            Some(output) => PathBuf::from(output),
            None => {
                error!("Output directory is required");
                println!("Output directory is required");
                anyhow::bail!("Output directory is required");
            }
        }
    };

    // 确保输出目录存在
    debug!(?output_dir, "Ensuring output directory exists");
    ensure_dir_exists(&output_dir)?;
    Ok(output_dir)
}

fn backup_items(
    container_info: &ContainerInfo,
    output_dir: PathBuf,
    total_volumes_count: usize,
    selected_volumes: Vec<VolumeInfo>,
    exclude_patterns: &[&str],
) -> Result<()> {
    // 创建备份映射
    let backup_mapping = BackupMapping {
        container_name: container_info.name.clone(),
        container_id: container_info.id.clone(),
        volumes: selected_volumes.clone(),
        backup_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let mapping_content = toml::to_string(&backup_mapping)?;

    // 创建内存文件
    let memory_files = vec![(MAPPING_FILE_NAME, mapping_content.as_str())];

    // 创建备份文件名
    let middle_name = if total_volumes_count > selected_volumes.len() {
        "all"
    } else {
        "partial"
    };
    let backup_filename = create_timestamp_filename(
        &format!("{}_{}", container_info.name, middle_name),
        ".tar.xz",
    );

    // 创建备份路径
    let backup_path = output_dir.join(&backup_filename);

    // 获取卷源路径
    let volumes_source = selected_volumes
        .iter()
        .map(|v| v.source.as_path())
        .collect::<Vec<_>>();

    // 压缩卷目录
    utils::compress_with_memory_file(
        &volumes_source,
        &backup_path,
        &memory_files,
        exclude_patterns,
    )?;

    // 备份完成
    info!(
        backup_file = ?backup_path,
        "All volumes backup completed successfully"
    );
    println!("All volumes backup completed: {}", backup_path.display());
    Ok(())
}

async fn select_volumes(
    file: Option<String>,
    interactive: bool,
    client: &DockerClient,
    container_info: &ContainerInfo,
) -> Result<(usize, Vec<VolumeInfo>)> {
    #[allow(unused_assignments)]
    let total_volumes: usize;
    #[allow(unused_assignments)]
    let mut selected_volumes = Vec::new();

    if let Some(file) = file {
        let file = PathBuf::from(file);
        if !file.exists() {
            error!("File does not exist: {}", file.display());
            println!("File does not exist: {}", file.display());
            anyhow::bail!("File does not exist: {}", file.display());
        }

        total_volumes = 1;
        // 获取卷信息
        selected_volumes = vec![VolumeInfo {
            source: file.clone(),
            destination: file.clone(),
            name: file
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
            anyhow::bail!("No volumes found for container {}", container_info.name);
        }

        // 选择要备份的卷
        debug!(volume_count = volumes.len(), "Selecting volumes to backup");
        total_volumes = volumes.len();
        selected_volumes = if interactive {
            select_volumes_prompt(&volumes)?
        } else {
            volumes
        };
    }

    if selected_volumes.is_empty() {
        error!("No volumes selected for backup");
        println!("No volumes selected for backup");
        anyhow::bail!("No volumes selected for backup");
    }

    Ok((total_volumes, selected_volumes))
}

// TODO
pub async fn restore(
    container: Option<String>,
    input: Option<String>,
    output: Option<String>,
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
    let config = Config::global()?;

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        prompt::select_container_prompt(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取备份文件路径
    debug!(container_name = ?container_info.name, "Getting backup file path");
    let file_path = if interactive || input.is_none() {
        let bkp_dir = config.backup_dir;

        let files = utils::get_files_start_with(&bkp_dir, &container_info.name, true)?;

        if files.is_empty() {
            error!("No backup files found");
            println!("No backup files found");
            return Ok(());
        }

        if files.len() == 1 {
            files[0].clone()
        } else {
            let selection = Select::new()
                .with_prompt(prompt_select!("Select one file to restore"))
                .items(&files.iter().map(|f| f.display()).collect::<Vec<_>>())
                .default(0)
                .interact()?;
            files[selection].clone()
        }
    } else {
        match input {
            Some(input) => PathBuf::from(input),
            None => {
                error!("Backup file path is required");
                println!("Backup file path is required");
                return Ok(());
            }
        }
    };

    // 确保备份文件存在
    debug!(file_path = ?file_path, "Ensuring backup file exists");
    if !file_path.exists() {
        error!("Backup file does not exist: {}", file_path.display());
        println!("Backup file does not exist: {}", file_path.display());
        return Ok(());
    }

    // 如果容器正在运行或重启中，则停止容器
    stop_container_timeout(&client, &container_info, timeout).await?;

    // Read mapping from backup file
    let mapping_content = utils::read_file_from_archive(&file_path, "mapping.toml")?;
    let backup_mapping: BackupMapping = toml::from_str(&mapping_content)?;

    // Verify container matches
    if container_info.name != backup_mapping.container_name {
        error!(
            "Backup is for container {} but trying to restore to {}",
            backup_mapping.container_name, container_info.name
        );
        return Err(anyhow::anyhow!(
            "Container mismatch: backup is for {} but trying to restore to {}",
            backup_mapping.container_name,
            container_info.name
        ));
    }

    // If output path not specified, use the original paths from mapping
    let output_dir = if let Some(output) = output {
        PathBuf::from(output)
    } else {
        // Use the first volume's source path from mapping
        let user_input = Input::new()
            .with_prompt("Restore output directory")
            .default(
                backup_mapping.volumes[0]
                    .source
                    .to_string_lossy()
                    .to_string(),
            )
            .allow_empty(false)
            .interact_text()?;
        PathBuf::from(user_input)
    };

    // 确保输出目录存在
    debug!(?output_dir, "Ensuring output directory exists");
    ensure_dir_exists(&output_dir)?;

    // 确认恢复
    if interactive {
        debug!("Requesting user confirmation");
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Are you sure you want to restore {} to container {}?",
                file_path.display(),
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
    restore_volume(&container_info, &file_path, &output_dir).await?;

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

    // TODO
    let mapping_content = toml::to_string("&backup_mapping")?;

    // 压缩卷目录
    utils::compress_with_memory_file(
        &[&volume.source],
        &backup_path,
        &[("mapping.toml", &mapping_content)],
        &[".git", "node_modules", "target"],
    )?;

    info!(
        backup_file = ?backup_path,
        "Volume backup completed successfully"
    );
    println!("Backup completed: {}", backup_path.display());
    Ok(())
}

async fn restore_volume(
    container: &ContainerInfo,
    file_path: &PathBuf,
    output_dir: &PathBuf,
) -> Result<()> {
    info!(
        container_name = ?container.name,
        file_path = ?file_path,
        "Starting volume restore"
    );

    println!(
        "Restoring {} to container {}",
        file_path.display(),
        container.name
    );

    debug!(
        source = ?file_path,
        destination = ?output_dir,
        "Extracting backup archive"
    );

    // 解压备份文件到卷目录
    extract_archive(file_path, output_dir)?;

    info!("Volume restore completed successfully");
    println!("Restore completed to {}", output_dir.display());
    Ok(())
}
