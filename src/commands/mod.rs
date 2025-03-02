mod prompt;

use crate::config::Config;
use crate::docker::{
    BackupMapping, ContainerInfo, DockerClient, DockerClientInterface, VolumeInfo,
};
use crate::utils::{self, create_timestamp_filename, ensure_dir_exists, unpack_archive};
use crate::{log_bail, log_println};
use prompt::*;

use anyhow::Result;
use chrono::Local;
use dialoguer::{Confirm, Input, Select};
use fs_extra;
use std::io::Write;
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
    let client = DockerClient::global()?;

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

async fn stop_container_timeout(container_info: &ContainerInfo) -> Result<()> {
    // 首先尝试停止容器
    println!("Attempting to stop container {}", container_info.name);
    let client = DockerClient::global()?;

    let stop_timeout_secs = client.get_stop_timeout_secs();
    debug!(
        "Attempting to stop container {} with timeout {}",
        container_info.id, stop_timeout_secs
    );

    let stop_task = client.stop_container(&container_info.id);

    // 然后等待容器完全停止，并添加终端输出反馈
    let timer_task = tokio::time::timeout(Duration::from_secs(stop_timeout_secs), async {
        let mut flag = 0;
        loop {
            match client.get_container_status(&container_info.id).await {
                Ok(status) => {
                    if status != "running" && status != "restarting" {
                        log_println!(
                            "INFO",
                            "Container {} stopped successfully with status: {}",
                            container_info.name,
                            status
                        );
                        return Ok(()); // 容器成功停止，返回 Ok
                    } else {
                        if flag == 0 {
                            log_println!(
                                "INFO",
                                "Container {} still stopping, current status: {}",
                                container_info.name,
                                status
                            );
                        }

                        print!(".");
                        std::io::stdout().flush()?;
                        flag = 1;
                    }
                }
                Err(e) => {
                    // 获取状态失败，返回 Err
                    log_bail!(
                        "ERROR",
                        "Failed to get container status for container {}: {}",
                        container_info.name,
                        e
                    );
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await; // 每秒检查一次状态
        }
    });

    tokio::select! {
        stop_res = stop_task => {
            println!();
            match stop_res {
                Ok(_) => Ok(()),
                Err(e) => {
                    log_bail!("ERROR", "Failed to stop container {}: {}", container_info.name, e);
                }
            }
        }
        timer_res = timer_task => {
            println!();
            // 处理超时情况和结果
            match timer_res {
                Ok(result) => result.map_err(|e| {
                    anyhow::anyhow!("Failed to stop container {}: {}", container_info.name, e)
                }),
                Err(_timeout_err) => {
                    // _timeout_err 是 tokio::time::error::Elapsed 类型的错误
                    log_bail!(
                        "ERROR",
                        "Timeout while waiting for container {} to stop after {} seconds",
                        container_info.name,
                        stop_timeout_secs
                    );
                }
            }
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
) -> Result<()> {
    // 获取全局配置
    let config = Config::global()?;
    let interactive = config.interactive;
    let restart = config.restart;
    let exclude_patterns = config.get_exclude_patterns();

    info!(
        ?container,
        ?file,
        ?output,
        restart,
        interactive,
        "Starting backup operation"
    );

    let client = DockerClient::global()?;

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        prompt::select_container_prompt(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取输出目录
    let output_dir = parse_output_dir(output, interactive, &container_info)?;

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
        &exclude_patterns,
    )
    .await?;

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

/// 获取输出目录
fn parse_output_dir(
    output: Option<String>,
    interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    debug!(container_name = ?container_info.name, "Getting output directory");
    let config = Config::global()?;

    // 如果指定了输出目录，则直接使用该目录
    if let Some(output) = output {
        let output_dir = PathBuf::from(output);
        ensure_dir_exists(&output_dir)?;
        // 将输出目录转换为绝对路径
        let output_dir = utils::absolute_canonicalize_path(&output_dir)?;
        return Ok(output_dir);
    }

    // 如果未指定输出目录，则交互式获取输出目录
    if interactive {
        let default_dir = config.backup_dir.to_string_lossy().to_string();

        let input: String = Input::new()
            .with_prompt("Backup output directory")
            .default(default_dir)
            .allow_empty(false)
            .interact_text()?;

        let output_dir = PathBuf::from(input);

        // 确保输出目录存在
        ensure_dir_exists(&output_dir)?;
        // 将输出目录转换为绝对路径
        let output_dir = utils::absolute_canonicalize_path(&output_dir)?;
        return Ok(output_dir);
    }

    // 如果未指定输出目录，则使用默认目录
    let output_dir = PathBuf::from(config.backup_dir);
    // 将输出目录转换为绝对路径
    let output_dir = utils::absolute_canonicalize_path(&output_dir)?;
    Ok(output_dir)
}

async fn backup_items(
    container_info: &ContainerInfo,
    output_dir: PathBuf,
    total_volumes_count: usize,
    selected_volumes: Vec<VolumeInfo>,
    exclude_patterns: &[&str],
) -> Result<()> {
    let selected_volumes = selected_volumes
        .into_iter()
        .filter(|v| {
            !exclude_patterns
                .iter()
                .any(|p| v.source.to_string_lossy().contains(p))
        })
        .collect::<Vec<_>>();

    // 如果备份卷为空，则直接返回
    if selected_volumes.is_empty() {
        log_bail!("ERROR", "No volumes for backup, please check your input");
    }

    // 如果备份卷为空，则直接返回
    // 创建备份映射
    // 创建备份映射
    let backup_mapping = BackupMapping {
        container_name: container_info.name.clone(),
        container_id: container_info.id.clone(),
        volumes: selected_volumes.clone(),
        backup_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // 序列化为 TOML
    let mapping_content = toml::to_string(&backup_mapping)?;

    // 创建备份文件名
    let middle_name = if total_volumes_count > selected_volumes.len() {
        "partial"
    } else {
        "all"
    };
    let backup_filename = create_timestamp_filename(
        &format!("{}_{}", container_info.name, middle_name),
        ".tar.xz",
    );
    let backup_path = output_dir.join(&backup_filename);

    // 获取卷源路径
    let volumes_source = selected_volumes
        .iter()
        .map(|v| v.source.as_path())
        .collect::<Vec<_>>();

    #[cfg(not(test))]
    {
        // 测试时，不停止容器 (不一定存在 Docker 环境)
        // 如果容器正在运行或重启中，则停止容器
        stop_container_timeout(&container_info).await?;
    }

    // 开始备份
    // 压缩卷目录，包含 mapping.toml
    utils::compress_with_memory_file(
        &volumes_source,
        &backup_path,
        &[(MAPPING_FILE_NAME, mapping_content.as_str())],
        exclude_patterns,
    )?;

    log_println!(
        "INFO",
        "Backup {} volumes completed: {}",
        selected_volumes.len(),
        backup_path.to_string_lossy()
    );
    Ok(())
}

async fn select_volumes<T: DockerClientInterface>(
    file: Option<String>,
    interactive: bool,
    client: &T,
    container_info: &ContainerInfo,
) -> Result<(usize, Vec<VolumeInfo>)> {
    // 处理单文件备份场景
    if let Some(file) = file {
        let file_path = PathBuf::from(file);
        // 将文件路径转换为绝对路径
        let file_path = utils::absolute_canonicalize_path(&file_path)?;
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
) -> Result<()> {
    let config = Config::global()?;
    let interactive = config.interactive;
    let restart = config.restart;
    let yes = config.yes;

    info!(
        ?container,
        ?input,
        restart,
        interactive,
        "Starting restore operation"
    );

    let client = DockerClient::global()?;

    // 获取容器信息
    debug!("Getting container information");
    let container_info = if interactive || container.is_none() {
        prompt::select_container_prompt(&client).await?
    } else {
        get_container_by_name_or_id(&client, &container.unwrap()).await?
    };

    // 获取备份文件路径
    let file_path = parse_restore_file(input, interactive, &container_info)?;

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

    // 如果指定了输出路径，直接使用该路径
    if let Some(output_path) = output {
        let output_path = PathBuf::from(output_path);
        ensure_dir_exists(&output_path)?;
        // 将输出路径转换为绝对路径
        let output_path = utils::absolute_canonicalize_path(&output_path)?;

        if !yes && interactive {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "❓ Are you sure you want to restore to {}?",
                    output_path.display()
                ))
                .default(true)
                .interact()?;

            if !confirmed {
                log_println!("INFO", "Restore cancelled");
                return Ok(());
            }
        }

        #[cfg(not(test))]
        {
            // 测试时，不停止容器 (不一定存在 Docker 环境)
            // 如果容器正在运行或重启中，则停止容器
            stop_container_timeout(&container_info).await?;
        }

        // 开始解压
        unpack_archive_to(container_info, file_path, &output_path).await?;
        return Ok(());
    }

    // 否则使用原始路径恢复
    if !yes && interactive {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "❓ Are you sure you want to restore to original paths?\n{}",
                backup_mapping
                    .volumes
                    .iter()
                    .map(|v| format!(" - {} -> {}", v.name, v.source.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
            .default(true)
            .interact()?;

        if !confirmed {
            log_println!("INFO", "Restore cancelled");
            return Ok(());
        }
    }

    #[cfg(not(test))]
    {
        // 测试时，不停止容器 (不一定存在 Docker 环境)
        // 如果容器正在运行或重启中，则停止容器
        stop_container_timeout(&container_info).await?;
    }

    // 开始解压
    unpack_archive_move(container_info, file_path, &backup_mapping.volumes).await?;
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
    _interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    let config = Config::global()?;
    debug!(container_name = ?container_info.name, "Getting backup file path");

    // 如果提供了输入路径，处理输入路径，直接返回
    if let Some(input) = input {
        let file = PathBuf::from(input);
        let file = utils::ensure_file_exists(&file)?;
        // 将文件路径转换为绝对路径
        let file = utils::absolute_canonicalize_path(&file)?;
        return Ok(file);
    }

    // 从备份目录查找文件
    let files = utils::get_files_start_with(&config.backup_dir, &container_info.name, true)?;

    // 如果找不到备份文件，则提示用户输入备份文件路径
    if files.is_empty() {
        log_println!(
            "WARN",
            "❌ No backup files found for container {}",
            container_info.name
        );
        let input = Input::new()
            .with_prompt("Please input the backup file path")
            .allow_empty(false)
            .validate_with(|input: &String| -> anyhow::Result<()> {
                let file = PathBuf::from(input);
                if !file.exists() {
                    Err(anyhow::anyhow!(
                        "File does not exist: {}",
                        file.to_string_lossy()
                    ))
                } else {
                    Ok(())
                }
            })
            .with_initial_text(config.backup_dir.to_string_lossy().to_string())
            .interact_text()?;
        let file = PathBuf::from(input);
        let file = utils::ensure_file_exists(&file)?;

        // 将文件路径转换为绝对路径
        let file = utils::absolute_canonicalize_path(&file)?;
        return Ok(file);
    }

    // 如果只有一个文件或需要选择
    let file = if files.len() == 1 {
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
    };

    // 将文件路径转换为绝对路径
    let file = utils::absolute_canonicalize_path(&file)?;
    Ok(file)
}

async fn get_container_by_name_or_id<T: DockerClientInterface>(
    client: &T,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::{
        ClientType, DOCKER_CLIENT_INSTANCE, DockerClient, DockerClientInterface,
        MockDockerClientInterface,
    };
    use assert_fs::TempDir;
    use predicates::ord::eq;

    use std::{
        fs,
        sync::{Arc, RwLock},
    };
    use tokio::test;

    #[test]
    async fn test_list_containers_with_mock() -> Result<()> {
        // 1. 创建 MockDockerClientInterface 实例
        let mut mock_client = MockDockerClientInterface::new();

        // 2. 设置 Mock 对象的行为 (expectations)
        mock_client.expect_list_containers().returning(|| {
            // 模拟 list_containers 方法的返回值
            Ok(vec![
                ContainerInfo {
                    id: "test_id_1".to_string(),
                    name: "test_container_1".to_string(),
                    status: "running".to_string(),
                },
                ContainerInfo {
                    id: "test_id_2".to_string(),
                    name: "test_container_2".to_string(),
                    status: "exited".to_string(),
                },
            ])
        });

        // 3.  替换全局 DockerClient 实例 (仅在测试中有效!)
        DOCKER_CLIENT_INSTANCE
            .set(Arc::new(RwLock::new(mock_client)))
            .unwrap();

        // 4. 调用 DockerClient::global() 获取 Mock 实例
        let client = DockerClient::global()?;

        // 5. 调用被测试的代码 (例如 list_containers)
        let containers = client.list_containers().await?;

        // 6.  断言测试结果
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0].name, "test_container_1");
        assert_eq!(containers[1].status, "exited");

        println!("Test finished with Mock DockerClient.");
        Ok(())
    }

    #[test]
    async fn test_get_container_volumes_with_mock() -> Result<()> {
        let mut mock_client = MockDockerClientInterface::new();

        mock_client
            .expect_get_container_volumes()
            .with(eq("test_container_id")) // 期望 get_container_volumes 被调用时 container_id 参数为 "test_container_id"
            .returning(|_| {
                // 模拟 get_container_volumes 的返回值
                Ok(vec![VolumeInfo {
                    name: "test_volume_1".to_string(),
                    source: PathBuf::from("/source/path1"),
                    destination: PathBuf::from("/destination/path1"),
                }])
            });

        DOCKER_CLIENT_INSTANCE
            .set(Arc::new(RwLock::new(mock_client)))
            .unwrap();
        let client = DockerClient::global()?;

        let volumes = client.get_container_volumes("test_container_id").await?;

        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "test_volume_1");
        assert_eq!(volumes[0].source, PathBuf::from("/source/path1"));

        println!("Test finished for get_container_volumes with Mock.");
        Ok(())
    }

    // 辅助函数：初始化 docker client
    fn setup_docker_client() -> Result<()> {
        DockerClient::init(10)?;
        Ok(())
    }

    // 辅助函数：创建测试用的卷目录和文件
    async fn setup_test_volumes() -> Result<(TempDir, Vec<VolumeInfo>)> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        // 创建测试卷目录
        let volumes = vec![
            ("vol1", "test1.txt", "content1"),
            ("vol2", "test2.txt", "content2"),
        ];

        let mut volume_infos = Vec::new();
        for (vol_name, file_name, content) in volumes {
            let vol_path = base_path.join(vol_name);
            fs::create_dir(&vol_path)?;
            fs::write(vol_path.join(file_name), content)?;

            volume_infos.push(VolumeInfo {
                name: vol_name.to_string(),
                source: vol_path.clone(),
                destination: vol_path,
            });
        }

        setup_docker_client()?;

        Ok((temp_dir, volume_infos))
    }

    // 测试备份功能
    #[test]
    async fn test_backup_items() -> Result<()> {
        let (_temp_dir, volumes) = setup_test_volumes().await?;
        let output_dir = TempDir::new()?;

        let container_info = ContainerInfo {
            id: "test_container_id".to_string(),
            name: "test_container".to_string(),
            status: "running".to_string(),
        };

        // 执行备份
        backup_items(
            &container_info,
            output_dir.path().to_path_buf(),
            volumes.len(),
            volumes,
            &[],
        )
        .await?;

        // 验证备份文件是否创建
        let backup_files: Vec<_> = fs::read_dir(output_dir.path())?
            .filter_map(|entry| entry.ok())
            .collect();
        assert_eq!(backup_files.len(), 1);

        // 验证备份文件是否包含 mapping.toml
        let backup_file = &backup_files[0].path();
        let mapping_content = utils::read_file_from_archive(backup_file, MAPPING_FILE_NAME)?;
        let backup_mapping: BackupMapping = toml::from_str(&mapping_content)?;

        assert_eq!(backup_mapping.container_name, "test_container");
        assert_eq!(backup_mapping.container_id, "test_container_id");
        assert_eq!(backup_mapping.volumes.len(), 2);

        Ok(())
    }

    // 测试恢复功能
    #[test]
    async fn test_restore_volumes() -> Result<()> {
        // Create and backup test data
        let (source_temp_dir, volumes) = setup_test_volumes().await?;
        let backup_dir = TempDir::new()?;
        let restore_dir = TempDir::new()?;

        let container_info = ContainerInfo {
            id: "test_container_id".to_string(),
            name: "test_container".to_string(),
            status: "running".to_string(),
        };

        // Backup
        backup_items(
            &container_info,
            backup_dir.path().to_path_buf(),
            volumes.len(),
            volumes.clone(),
            &[],
        )
        .await?;

        // Get backup file
        let backup_file = fs::read_dir(backup_dir.path())?.next().unwrap()?.path();

        // Restore to specified directory
        restore_volumes(
            &container_info,
            &backup_file,
            Some(restore_dir.path().to_string_lossy().to_string()),
            false,
            true,
        )
        .await?;

        // Verify restored files
        for volume in volumes {
            let source_path = volume.source;
            let relative_path = source_path.strip_prefix(source_temp_dir.path())?;
            let restore_path = restore_dir.path().join(relative_path);

            assert!(restore_path.exists());

            // Get the test file name based on volume name
            let test_file = format!("test{}.txt", volume.name.strip_prefix("vol").unwrap());
            assert!(restore_path.join(&test_file).exists());

            // Verify content
            let original_content = fs::read_to_string(source_path.join(&test_file))?;
            let restored_content = fs::read_to_string(restore_path.join(&test_file))?;
            assert_eq!(original_content, restored_content);
        }

        Ok(())
    }

    // 测试容器不匹配的情况
    #[test]
    async fn test_restore_container_mismatch() -> Result<()> {
        let (_source_temp_dir, volumes) = setup_test_volumes().await?;
        let backup_dir = TempDir::new()?;

        // 使用一个容器创建备份
        let container_info = ContainerInfo {
            id: "test_container_id".to_string(),
            name: "test_container".to_string(),
            status: "running".to_string(),
        };

        backup_items(
            &container_info,
            backup_dir.path().to_path_buf(),
            volumes.len(),
            volumes,
            &[],
        )
        .await?;

        let backup_file = fs::read_dir(backup_dir.path())?.next().unwrap()?.path();

        // 使用不同的容器尝试恢复
        let different_container = ContainerInfo {
            id: "different_id".to_string(),
            name: "different_container".to_string(),
            status: "running".to_string(),
        };

        // 应该返回错误
        let result = restore_volumes(&different_container, &backup_file, None, false, true).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Backup is for container")
        );

        Ok(())
    }

    // 测试排除模式
    #[test]
    async fn test_backup_with_exclude_patterns() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        // Create test file structure
        fs::create_dir_all(base_path.join("vol1/node_modules"))?;
        fs::create_dir_all(base_path.join("vol1/.git"))?;
        fs::write(base_path.join("vol1/test.txt"), "test content")?;
        fs::write(
            base_path.join("vol1/node_modules/package.json"),
            "package content",
        )?;
        fs::write(base_path.join("vol1/.git/config"), "git config")?;

        let volumes = vec![VolumeInfo {
            name: "vol1".to_string(),
            source: base_path.join("vol1"),
            destination: base_path.join("vol1"),
        }];

        let container_info = ContainerInfo {
            id: "test_container_id".to_string(),
            name: "test_container".to_string(),
            status: "running".to_string(),
        };

        let output_dir = TempDir::new()?;

        // Backup with exclude patterns
        backup_items(
            &container_info,
            output_dir.path().to_path_buf(),
            volumes.len(),
            volumes,
            &[".git", "node_modules"],
        )
        .await?;

        // Get backup file
        let backup_file = fs::read_dir(output_dir.path())?.next().unwrap()?.path();

        // Create restore directory
        let restore_dir = TempDir::new()?;

        // Restore backup
        restore_volumes(
            &container_info,
            &backup_file,
            Some(restore_dir.path().to_string_lossy().to_string()),
            false,
            true,
        )
        .await?;

        // Verify restored structure
        assert!(restore_dir.path().join("vol1").exists());
        assert!(restore_dir.path().join("vol1/test.txt").exists());
        assert!(!restore_dir.path().join("vol1/node_modules").exists());
        assert!(!restore_dir.path().join("vol1/.git").exists());

        // Verify content
        let content = fs::read_to_string(restore_dir.path().join("vol1/test.txt"))?;
        assert_eq!(content, "test content");

        Ok(())
    }
}
