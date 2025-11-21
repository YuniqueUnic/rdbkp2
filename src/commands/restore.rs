use crate::{
    commands::{MAPPING_FILE_NAME, container, prompt},
    config::Config,
    docker::{BackupMapping, ContainerInfo, DockerClient, DockerClientInterface, VolumeInfo},
    log_bail, log_println,
    utils::{self, ensure_dir_exists, unpack_archive},
};

use anyhow::Result;
use dialoguer::{Confirm, Input, Select};
use std::path::PathBuf;
use tempfile::tempdir;
use toml;
use tracing::{info, warn};

use super::privileges;

pub async fn restore(
    container: Option<String>,
    input: Option<String>,
    output: Option<String>,
) -> Result<()> {
    prompt::require_admin_privileges_prompt()?;

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
    let container_info = container::select_container(&client, container, interactive).await?;
    let file_path = parse_restore_file(input, interactive, &container_info)?;

    restore_volumes(
        &client,
        &container_info,
        &file_path,
        output,
        interactive,
        yes,
    )
    .await?;

    if restart {
        log_println!(
            "INFO",
            "{}",
            t!(
                "commands.restarting_container",
                "name" = container_info.name
            )
        );
        client.restart_container(&container_info.id).await?;
        log_println!(
            "INFO",
            "{}",
            t!("commands.container_restarted", "name" = container_info.name)
        );
    }

    Ok(())
}

async fn restore_volumes<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
    file_path: &PathBuf,
    output: Option<String>,
    interactive: bool,
    yes: bool,
) -> Result<()> {
    let mapping_content = utils::read_file_from_archive(file_path, MAPPING_FILE_NAME)?;
    let backup_mapping: BackupMapping = toml::from_str(&mapping_content)?;

    if container_info.name != backup_mapping.container_name {
        log_bail!(
            "ERROR",
            "{}",
            t!(
                "commands.backup_is_for_container",
                "backup_container" = backup_mapping.container_name,
                "restore_container" = container_info.name
            )
        );
    }

    if let Some(output_path) = output {
        return restore_to_directory(
            client,
            container_info,
            file_path,
            output_path,
            interactive,
            yes,
        )
        .await;
    }

    restore_in_place(
        client,
        container_info,
        file_path,
        &backup_mapping.volumes,
        interactive,
        yes,
    )
    .await
}

async fn restore_to_directory<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
    file_path: &PathBuf,
    output_path: String,
    interactive: bool,
    yes: bool,
) -> Result<()> {
    let output_path = PathBuf::from(output_path);
    ensure_dir_exists(&output_path)?;
    let output_path = utils::absolute_canonicalize_path(&output_path)?;

    if !yes && interactive {
        let confirmed = Confirm::new()
            .with_prompt(t!(
                "commands.are_you_sure_you_want_to_restore_to",
                "path" = output_path.display()
            ))
            .default(true)
            .interact()?;

        if !confirmed {
            log_println!("INFO", "{}", t!("prompt.restore_cancelled"));
            return Ok(());
        }
    }

    container::ensure_container_stopped(client, container_info).await?;
    unpack_archive_to(container_info, file_path, &output_path).await
}

async fn restore_in_place<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
    file_path: &PathBuf,
    volumes: &[VolumeInfo],
    interactive: bool,
    yes: bool,
) -> Result<()> {
    if !yes && interactive {
        let prompt_text = volumes
            .iter()
            .map(|v| format!(" - {} -> {}", v.name, v.source.display()))
            .collect::<Vec<_>>()
            .join("\n");

        let confirmed = Confirm::new()
            .with_prompt(t!(
                "commands.are_you_sure_you_want_to_restore_to",
                "path" = prompt_text
            ))
            .default(true)
            .interact()?;

        if !confirmed {
            log_println!("INFO", "{}", t!("prompt.restore_cancelled"));
            return Ok(());
        }
    }

    container::ensure_container_stopped(client, container_info).await?;
    unpack_archive_move(container_info, file_path, volumes).await
}

fn parse_restore_file(
    input: Option<String>,
    interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    let config = Config::global()?;

    fn try_get_backup_file(path: &PathBuf, container_name: &str) -> Result<Option<PathBuf>> {
        if path.is_file() {
            let file = utils::ensure_file_exists(path)?;
            return Ok(Some(utils::absolute_canonicalize_path(&file)?));
        }

        if path.is_dir() {
            let mut files = utils::get_files_start_with(path, container_name, true)?;
            if files.is_empty() {
                return Ok(None);
            }
            if files.len() == 1 {
                return Ok(Some(utils::absolute_canonicalize_path(&files[0])?));
            }

            files.sort_by(|a, b| {
                let created = |p: &PathBuf| {
                    std::fs::metadata(p)
                        .and_then(|m| m.created())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                };
                created(b).cmp(&created(a))
            });

            let selection = Select::new()
                .with_prompt(prompt::prompt_select(&format!(
                    "{}",
                    t!("commands.select_backup_file_to_restore")
                )))
                .items(
                    &files
                        .iter()
                        .map(|f| {
                            format!(
                                "[{:<19}] {:<45}",
                                utils::format_file_time(f)
                                    .unwrap_or_else(|_| "Unknown".to_string()),
                                f.file_name().unwrap_or_default().to_string_lossy()
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .default(0)
                .interact()?;

            return Ok(Some(utils::absolute_canonicalize_path(&files[selection])?));
        }

        Ok(None)
    }

    if let Some(input) = input {
        let input_path = PathBuf::from(input);
        if let Some(file) = try_get_backup_file(&input_path, &container_info.name)? {
            return Ok(file);
        }
    }

    if let Some(file) = try_get_backup_file(&config.backup_dir, &container_info.name)? {
        return Ok(file);
    }

    if interactive {
        log_println!(
            "WARN",
            "{}",
            t!(
                "commands.no_backup_files_found_for_container",
                "container_name" = container_info.name
            )
        );

        let input: String = Input::new()
            .with_prompt(t!("prompt.backup_file_path_input_prompt"))
            .allow_empty(false)
            .with_initial_text(config.backup_dir.to_string_lossy().to_string())
            .interact_text()?;

        let input_path = PathBuf::from(input);
        if let Some(file) = try_get_backup_file(&input_path, &container_info.name)? {
            return Ok(file);
        }
    }

    log_bail!(
        "ERROR",
        "{}",
        t!(
            "commands.could_not_find_valid_backup_file_for_container",
            "container_name" = container_info.name
        )
    )
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
        "Restoring archive to directory"
    );

    println!(
        "{}",
        t!(
            "commands.restoring_to",
            "file_path" = file_path.to_string_lossy(),
            "output_dir" = output_dir.to_string_lossy()
        )
    );

    unpack_archive(file_path, output_dir)?;
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
        "Restoring archive into volume mounts"
    );

    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path().to_path_buf();
    unpack_archive(file_path, &temp_path)?;

    for volume in volumes {
        let temp_source = temp_path.join(&volume.name);
        if !temp_source.exists() {
            warn!(volume = ?volume.name, "Volume not found in backup, skipping");
            continue;
        }

        println!(
            "Restoring volume {} to {}",
            volume.name,
            volume.source.to_string_lossy()
        );

        privileges::privileged_copy(&temp_source, &volume.source)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::{
        TempDir,
        fixture::{PathChild, PathCreateDir},
    };
    use std::fs;

    async fn setup_backup() -> Result<(TempDir, PathBuf, ContainerInfo)> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        fs::create_dir_all(base_path.join("vol1"))?;
        fs::write(base_path.join("vol1/data.txt"), "hello")?;

        let volumes = vec![VolumeInfo {
            name: "vol1".into(),
            source: base_path.join("vol1"),
            destination: base_path.join("vol1"),
        }];

        let container = ContainerInfo {
            id: "id".into(),
            name: "container".into(),
            status: "running".into(),
        };

        let output_dir = temp_dir.child("backup");
        output_dir.create_dir_all()?;

        let mapping = BackupMapping {
            container_name: container.name.clone(),
            container_id: container.id.clone(),
            volumes: volumes.clone(),
            backup_time: "now".into(),
            version: "test".into(),
        };

        let mapping_content = toml::to_string(&mapping)?;
        let backup_file = output_dir.child("backup.tar.xz");
        let sources: Vec<_> = volumes.iter().map(|v| v.source.as_path()).collect();
        crate::utils::compress_with_memory_file(
            &sources,
            backup_file.path(),
            &[(MAPPING_FILE_NAME, mapping_content.as_str())],
            &[],
        )?;

        Ok((temp_dir, backup_file.path().to_path_buf(), container))
    }

    #[tokio::test]
    async fn restore_to_custom_directory() -> Result<()> {
        DockerClient::init(10)?;
        let (_temp_dir, backup_file, container) = setup_backup().await?;
        let restore_dir = TempDir::new()?;

        let mut client = DockerClient::global()?;
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));

        client
            .expect_stop_container()
            .returning(|_| Ok(()))
            .times(0..=1);
        client
            .expect_get_stop_timeout_secs()
            .returning(|| 10)
            .times(0..=1);

        restore_volumes(
            &client,
            &container,
            &backup_file,
            Some(restore_dir.path().to_string_lossy().to_string()),
            false,
            true,
        )
        .await?;

        assert!(restore_dir.path().join("vol1/data.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn detect_container_mismatch() -> Result<()> {
        DockerClient::init(10)?;
        let (_temp_dir, backup_file, _container) = setup_backup().await?;
        let mut client = DockerClient::global()?;
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));

        let other_container = ContainerInfo {
            id: "other".into(),
            name: "other".into(),
            status: "running".into(),
        };

        let result =
            restore_volumes(&client, &other_container, &backup_file, None, false, true).await;

        assert!(result.is_err());
        Ok(())
    }
}
