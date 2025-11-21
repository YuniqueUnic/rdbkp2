use crate::{
    commands::{MAPPING_FILE_NAME, container, prompt},
    config::Config,
    docker::{BackupMapping, ContainerInfo, DockerClient, DockerClientInterface, VolumeInfo},
    log_bail, log_println,
    utils::{self, create_timestamp_filename, ensure_dir_exists},
};

use anyhow::Result;
use chrono::Local;
use dialoguer::Input;
use std::path::PathBuf;
use toml;
use tracing::{debug, info};

pub async fn backup(
    container: Option<String>,
    file: Option<String>,
    output: Option<String>,
) -> Result<()> {
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
    let container_info = container::select_container(&client, container, interactive).await?;

    let output_dir = parse_output_dir(output, interactive, &container_info)?;
    let (total_volumes, selected_volumes) =
        select_volumes(file, interactive, &client, &container_info).await?;

    perform_backup(
        &client,
        &container_info,
        output_dir,
        total_volumes,
        selected_volumes,
        &exclude_patterns,
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

fn parse_output_dir(
    output: Option<String>,
    interactive: bool,
    container_info: &ContainerInfo,
) -> Result<PathBuf> {
    debug!(container_name = ?container_info.name, "Resolving output directory");
    let config = Config::global()?;

    if let Some(output) = output {
        let output_dir = PathBuf::from(output);
        ensure_dir_exists(&output_dir)?;
        return Ok(utils::absolute_canonicalize_path(&output_dir)?);
    }

    if interactive {
        let default_dir = config.backup_dir.to_string_lossy().to_string();
        let input: String = Input::new()
            .with_prompt(t!("prompt.backup_out_dir_input_prompt"))
            .default(default_dir)
            .allow_empty(false)
            .interact_text()?;

        let output_dir = PathBuf::from(input);
        ensure_dir_exists(&output_dir)?;
        return Ok(utils::absolute_canonicalize_path(&output_dir)?);
    }

    Ok(utils::absolute_canonicalize_path(&config.backup_dir)?)
}

async fn select_volumes<T: DockerClientInterface>(
    file: Option<String>,
    interactive: bool,
    client: &T,
    container_info: &ContainerInfo,
) -> Result<(usize, Vec<VolumeInfo>)> {
    if let Some(file) = file {
        let file_path = PathBuf::from(file);
        let file_path = utils::absolute_canonicalize_path(&file_path)?;
        if !file_path.exists() {
            log_bail!(
                "ERROR",
                "{}",
                t!(
                    "commands.path_does_not_exist",
                    "path" = file_path.to_string_lossy()
                )
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

        debug!(volume = ?volume, "Single path backup configured");
        return Ok((1, vec![volume]));
    }

    debug!(container_id = ?container_info.id, "Fetching volumes for container");
    let volumes = client.get_container_volumes(&container_info.id).await?;

    if volumes.is_empty() {
        log_bail!(
            "ERROR",
            "{}",
            t!(
                "commands.no_volumes_found_for_container",
                "container_name" = container_info.name
            )
        );
    }

    let total_volumes = volumes.len();
    let selected_volumes = if interactive {
        prompt::select_volumes_prompt(&volumes)?
    } else {
        volumes
    };

    if selected_volumes.is_empty() {
        log_bail!("ERROR", "{}", t!("commands.no_volumes_selected_for_backup"));
    }

    Ok((total_volumes, selected_volumes))
}

async fn perform_backup<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
    output_dir: PathBuf,
    total_volumes_count: usize,
    selected_volumes: Vec<VolumeInfo>,
    exclude_patterns: &[&str],
) -> Result<()> {
    let filtered_volumes: Vec<_> = selected_volumes
        .into_iter()
        .filter(|v| {
            !exclude_patterns
                .iter()
                .any(|pattern| v.source.to_string_lossy().contains(pattern))
        })
        .collect();

    if filtered_volumes.is_empty() {
        log_bail!("ERROR", "{}", t!("commands.no_volumes_for_backup"));
    }

    let mapping = BackupMapping {
        container_name: container_info.name.clone(),
        container_id: container_info.id.clone(),
        volumes: filtered_volumes.clone(),
        backup_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let mapping_content = toml::to_string(&mapping)?;
    let middle_name = if total_volumes_count > filtered_volumes.len() {
        "partial"
    } else {
        "all"
    };
    let backup_filename = create_timestamp_filename(
        &format!("{}_{}", container_info.name, middle_name),
        ".tar.xz",
    );
    let backup_path = output_dir.join(&backup_filename);

    let sources = filtered_volumes
        .iter()
        .map(|v| v.source.as_path())
        .collect::<Vec<_>>();

    container::ensure_container_stopped(client, container_info).await?;

    utils::compress_with_memory_file(
        &sources,
        &backup_path,
        &[(MAPPING_FILE_NAME, mapping_content.as_str())],
        exclude_patterns,
    )?;

    log_println!(
        "INFO",
        "{}",
        t!(
            "commands.backup_volumes_completed",
            "volumes_count" = filtered_volumes.len(),
            "backup_path" = backup_path.to_string_lossy()
        )
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use std::fs;

    async fn setup_test_volumes() -> Result<(TempDir, Vec<VolumeInfo>)> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        let volumes = vec![
            ("vol1", "test1.txt", "content1"),
            ("vol2", "test2.txt", "content2"),
        ];

        let mut infos = Vec::new();
        for (name, file, content) in volumes {
            let vol_path = base_path.join(name);
            fs::create_dir(&vol_path)?;
            fs::write(vol_path.join(file), content)?;
            infos.push(VolumeInfo {
                name: name.to_string(),
                source: vol_path.clone(),
                destination: vol_path,
            });
        }

        DockerClient::init(10)?;
        Ok((temp_dir, infos))
    }

    #[tokio::test]
    async fn creates_backup_archive() -> Result<()> {
        let (_dir, volumes) = setup_test_volumes().await?;
        let output_dir = TempDir::new()?;

        DockerClient::init(10)?;

        let container = ContainerInfo {
            id: "id".into(),
            name: "container".into(),
            status: "running".into(),
        };

        let mut client = DockerClient::global()?;
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));

        perform_backup(
            &client,
            &container,
            output_dir.path().to_path_buf(),
            volumes.len(),
            volumes,
            &[],
        )
        .await?;

        assert_eq!(
            fs::read_dir(output_dir.path())?
                .filter_map(|e| e.ok())
                .count(),
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn respects_exclude_patterns() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path();

        fs::create_dir_all(base_path.join("vol1/node_modules"))?;
        fs::create_dir_all(base_path.join("vol1/.git"))?;
        fs::write(base_path.join("vol1/test.txt"), "content")?;
        fs::write(base_path.join("vol1/node_modules/file"), "skip")?;

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

        let output_dir = TempDir::new()?;
        DockerClient::init(10)?;
        let mut client = DockerClient::global()?;
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));

        perform_backup(
            &client,
            &container,
            output_dir.path().to_path_buf(),
            volumes.len(),
            volumes,
            &[".git", "node_modules"],
        )
        .await?;

        let backup_file = fs::read_dir(output_dir.path())?.next().unwrap()?.path();
        let restore_dir = TempDir::new()?;
        let restore_path = restore_dir.path().to_path_buf();
        crate::utils::unpack_archive(&backup_file, &restore_path)?;

        assert!(restore_dir.path().join("vol1/test.txt").exists());
        assert!(!restore_dir.path().join("vol1/node_modules").exists());
        Ok(())
    }
}
