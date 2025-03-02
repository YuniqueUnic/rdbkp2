mod prompt;

use crate::config::Config;
use crate::docker::{BackupMapping, ContainerInfo, DockerClient, VolumeInfo};
use crate::utils::{self, create_timestamp_filename, ensure_dir_exists, unpack_archive};
use crate::{log_bail, log_print};
use prompt::*;

use anyhow::Result;
use chrono::Local;
use dialoguer::{Confirm, Input, Select};
use fs_extra;
use std::path::PathBuf;
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

/*
 * backup
 */

pub async fn backup(
    container: Option<String>,
    file: Option<String>,
    output: Option<String>,
    restart: bool,
    interactive: bool,
    timeout: u64,
    exclude_patterns: &[&str],
    yes: bool,
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
                log_bail!("ERROR", "Output directory is required");
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
        selected_volumes_len = ?selected_volumes.len(),
        "Volumes backup completed successfully"
    );
    println!(
        "Volumes backup completed: {}",
        backup_path.to_string_lossy()
    );
    Ok(())
}

async fn select_volumes(
    file: Option<String>,
    interactive: bool,
    client: &DockerClient,
    container_info: &ContainerInfo,
) -> Result<(usize, Vec<VolumeInfo>)> {
    // 处理单文件备份场景
    if let Some(file) = file {
        let file_path = PathBuf::from(file);
        if !file_path.exists() {
            log_bail!(
                "ERROR",
                "File does not exist: {}",
                file_path.to_string_lossy()
            );
        }

        let volume = VolumeInfo {
            source: file_path.clone(),
            destination: file_path.clone(),
            name: file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        };

        return Ok((1, vec![volume]));
    }

    // 处理容器卷备份场景
    debug!(container_id = ?container_info.id, "Getting volume information");
    let volumes = client.get_container_volumes(&container_info.id).await?;

    if volumes.is_empty() {
        log_bail!(
            "ERROR",
            "No volumes found for container {}",
            container_info.name
        );
    }

    debug!(volume_count = volumes.len(), "Selecting volumes to backup");
    let total_volumes = volumes.len();
    let selected_volumes = if interactive {
        select_volumes_prompt(&volumes)?
    } else {
        volumes
    };

    if selected_volumes.is_empty() {
        log_bail!("ERROR", "No volumes selected for backup");
    }

    Ok((total_volumes, selected_volumes))
}

/*
 * restore
 */

pub async fn restore(
    container: Option<String>,
    input: Option<String>,
    output: Option<String>,
    restart: bool,
    interactive: bool,
    timeout: u64,
    yes: bool,
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
        prompt::select_container_prompt(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取备份文件路径
    let file_path = parse_restore_file(input, interactive, &container_info)?;

    // 如果容器正在运行或重启中，则停止容器
    stop_container_timeout(&client, &container_info, timeout).await?;

    // 恢复卷 (s)
    restore_volumes(&container_info, &file_path, output, interactive, yes).await?;

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

async fn restore_volumes(
    container_info: &ContainerInfo,
    file_path: &PathBuf,
    output: Option<String>,
    interactive: bool,
    yes: bool,
) -> Result<()> {
    // 读取并验证备份映射文件
    let mapping_content = utils::read_file_from_archive(&file_path, MAPPING_FILE_NAME)?;
    let backup_mapping: BackupMapping = toml::from_str(&mapping_content)?;

    // 验证容器匹配
    if container_info.name != backup_mapping.container_name {
        log_bail!(
            "ERROR",
            "Backup is for container {} but trying to restore to {}",
            backup_mapping.container_name,
            container_info.name
        );
    }

    // 处理恢复路径
    let (volumes_to_restore, output_dirs) = match &output {
        // 如果指定了输出路径，直接使用所有卷并输出到指定路径
        Some(output_path) => {
            let output_path = PathBuf::from(output_path);
            (backup_mapping.volumes, vec![output_path])
        }
        // 如果没有指定输出路径，让用户选择要恢复的卷（如果是交互模式）
        None => {
            let selected_volumes = if interactive {
                select_volumes_prompt(&backup_mapping.volumes)?
            } else {
                backup_mapping.volumes
            };

            let output_dirs = selected_volumes
                .iter()
                .map(|v| v.source.clone())
                .collect::<Vec<_>>();

            (selected_volumes, output_dirs)
        }
    };

    // 确保输出目录存在
    debug!(?output_dirs, "Ensuring output directories exist");
    for output_dir in &output_dirs {
        ensure_dir_exists(output_dir)?;
    }

    // 如果不是强制模式且是交互模式，询问用户确认
    if !yes && interactive {
        debug!("Requesting user confirmation");
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "❓ Are you sure you want to restore {} to container {}?",
                file_path.to_string_lossy(),
                container_info.name
            ))
            .default(true)
            .interact()?;

        if !confirmed {
            log_print!("INFO", "Restore cancelled");
            return Ok(());
        }
    }

    // 执行恢复操作
    if output.is_some() {
        // 如果指定了输出路径，所有内容解压到同一目录
        unpack_archive_to(&container_info, &file_path, &output_dirs[0]).await?;
    } else {
        // 否则，按照原始卷的路径结构恢复
        unpack_archive_move(&container_info, &file_path, &volumes_to_restore).await?;
    }

    info!(
        container_name = ?container_info.name,
        "Restore operation completed successfully"
    );

    Ok(())
}

async fn unpack_archive_to(
    container: &ContainerInfo,
    file_path: &PathBuf,
    output_dir: &PathBuf,
) -> Result<()> {
    info!(
        container_name = ?container.name,
        file_path = ?file_path,
        output_dir = ?output_dir,
        "Starting volume restore"
    );

    println!(
        "Restoring {} to {}",
        file_path.to_string_lossy(),
        output_dir.to_string_lossy()
    );

    // 解压备份文件到指定目录
    unpack_archive(file_path, output_dir)?;

    info!(
        container_name = ?container.name,
        output_dir = ?output_dir,
        "Volume restore completed"
    );
    Ok(())
}

async fn unpack_archive_move(
    container: &ContainerInfo,
    file_path: &PathBuf,
    volumes: &[VolumeInfo],
) -> Result<()> {
    info!(
        container_name = ?container.name,
        file_path = ?file_path,
        "Starting volume restore"
    );

    // 需要使用临时目录的原因：
    // 1. tar.xz 文件需要完整解压后才能访问其中的文件
    // 2. 需要保证原子性操作，避免解压过程中出错导致数据不一致
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.into_path();

    // 先解压到临时目录
    debug!(temp_dir = ?temp_path, "Extracting to temporary directory");
    unpack_archive(file_path, &temp_path)?;

    // 对每个卷进行恢复
    for volume in volumes {
        let temp_source_path = temp_path.join(&volume.name);
        let target_path = &volume.source;

        if !temp_source_path.exists() {
            warn!(
                volume = ?volume.name,
                "Volume data not found in backup, skipping"
            );
            continue;
        }

        println!(
            "Restoring volume {} to {}",
            volume.name,
            target_path.to_string_lossy()
        );

        // 使用 fs_extra 来复制目录内容，提供更好的错误处理和进度反馈
        let copy_options = fs_extra::dir::CopyOptions {
            overwrite: true,
            skip_exist: false,
            content_only: true,
            ..Default::default()
        };

        debug!(
            from = ?temp_source_path,
            to = ?target_path,
            "Copying volume data"
        );

        fs_extra::dir::copy(&temp_source_path, target_path, &copy_options)
            .map_err(|e| anyhow::anyhow!("Failed to copy volume data: {}", e))?;

        info!(volume = ?volume.name, "Volume restored successfully");
    }

    info!(
        container_name = ?container.name,
        "Volumes restored successfully"
    );
    Ok(())
}

fn parse_restore_file(
    input: Option<String>,
    interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    let config = Config::global()?;
    debug!(container_name = ?container_info.name, "Getting backup file path");

    // 如果提供了输入路径且不是交互模式，直接返回
    if !interactive && input.is_some() {
        let file = PathBuf::from(input.unwrap());
        let file = utils::ensure_file_exists(&file)?;
        return Ok(file);
    }

    // 处理输入路径
    if let Some(input) = input {
        let file = PathBuf::from(input);
        let file = utils::ensure_file_exists(&file)?;
        return Ok(file);
    }

    // 从备份目录查找文件
    let files = utils::get_files_start_with(&config.backup_dir, &container_info.name, true)?;
    if files.is_empty() {
        log_bail!(
            "ERROR",
            "No backup files found for container {}",
            container_info.name
        );
    }

    // 如果只有一个文件或需要选择
    Ok(if files.len() == 1 {
        files[0].clone()
    } else {
        let selection = Select::new()
            .with_prompt(prompt_select!("Select one file to restore"))
            .items(
                &files
                    .iter()
                    .map(|f| f.to_string_lossy())
                    .collect::<Vec<_>>(),
            )
            .default(0)
            .interact()?;
        files[selection].clone()
    })
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
