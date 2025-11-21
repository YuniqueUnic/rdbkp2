use crate::{
    commands::prompt,
    docker::{ContainerInfo, DockerClient, DockerClientInterface},
    log_bail, log_println,
};

use anyhow::Result;
use dialoguer::{Input, Select};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

pub async fn list_containers() -> Result<()> {
    debug!("Listing Docker containers");
    let client = DockerClient::global()?;
    let containers = client.list_containers().await?;

    if containers.is_empty() {
        println!("{}", t!("commands.no_containers_available"));
        return Ok(());
    }

    print_container_table(&containers);
    info!(container_count = containers.len(), "Container list printed");
    Ok(())
}

pub async fn select_container<T: DockerClientInterface>(
    client: &T,
    container: Option<String>,
    interactive: bool,
) -> Result<ContainerInfo> {
    if container.is_none() && interactive {
        return prompt::select_container_prompt(client).await;
    }

    let Some(mut container_input) = container else {
        log_bail!(
            "ERROR",
            "{}",
            t!("commands.container_name_or_id_must_be_specified_in_non_interactive_mode")
        );
    };

    container_input = container_input.trim().to_string();
    if container_input.is_empty() {
        if interactive {
            return prompt::select_container_prompt(client).await;
        }
        log_bail!(
            "ERROR",
            "{}",
            t!("commands.container_name_or_id_must_be_specified_in_non_interactive_mode")
        );
    }

    let matches = client.find_containers(&container_input).await?;
    match matches.len() {
        0 => handle_no_matches(client, container_input, interactive).await,
        1 => Ok(matches[0].clone()),
        _ => handle_multiple_matches(matches, interactive),
    }
}

pub async fn ensure_container_stopped<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
) -> Result<()> {
    let status = client.get_container_status(&container_info.id).await?;
    if !is_running(&status) {
        debug!(
            container = ?container_info.name,
            status = ?status,
            "Container already stopped"
        );
        return Ok(());
    }

    log_println!(
        "INFO",
        "{}",
        t!(
            "commands.attempt_to_stop_container",
            "container_name" = container_info.name
        )
    );

    stop_container_with_timeout(client, container_info).await
}

fn handle_multiple_matches(
    matches: Vec<ContainerInfo>,
    interactive: bool,
) -> Result<ContainerInfo> {
    if !interactive {
        log_bail!("ERROR", "{}", t!("commands.multiple_matches_found"));
    }

    let selection = Select::new()
        .with_prompt(prompt::prompt_select(&format!(
            "{}",
            t!("commands.multiple_matches_found")
        )))
        .items(
            &matches
                .iter()
                .map(|c| format!("{} ({})", c.name, c.id))
                .collect::<Vec<_>>(),
        )
        .default(0)
        .interact()?;

    Ok(matches[selection].clone())
}

async fn handle_no_matches<T: DockerClientInterface>(
    client: &T,
    attempted: String,
    interactive: bool,
) -> Result<ContainerInfo> {
    log_println!(
        "WARN",
        "{}",
        t!("commands.no_container_matched", "name" = attempted)
    );

    let containers = client.list_containers().await?;
    if containers.is_empty() {
        log_bail!("ERROR", "{}", t!("commands.no_containers_available"));
    }
    print_container_table(&containers);

    if !interactive {
        log_bail!(
            "ERROR",
            "{}",
            t!("commands.container_name_or_id_must_be_specified_in_non_interactive_mode")
        );
    }

    let input: String = Input::new()
        .with_prompt(t!("commands.please_enter_a_valid_container_name_or_id"))
        .with_initial_text(attempted)
        .allow_empty(false)
        .interact_text()?;

    let matches = client.find_containers(&input).await?;
    if matches.is_empty() {
        log_bail!(
            "ERROR",
            "{}",
            t!("commands.no_container_matched", "name" = input)
        );
    }
    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }
    handle_multiple_matches(matches, true)
}

async fn stop_container_with_timeout<T: DockerClientInterface>(
    client: &T,
    container_info: &ContainerInfo,
) -> Result<()> {
    let timeout_secs = client.get_stop_timeout_secs();
    if let Err(err) = client.stop_container(&container_info.id).await {
        let status = client
            .get_container_status(&container_info.id)
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        if !is_running(&status) {
            warn!(
                container = ?container_info.name,
                error = ?err,
                "Container already stopped"
            );
        } else {
            log_bail!(
                "ERROR",
                "{}",
                t!(
                    "commands.stop_container_failed",
                    "name" = container_info.name,
                    "error" = err
                )
            );
        }
    }

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let status = client.get_container_status(&container_info.id).await?;
        if !is_running(&status) {
            log_println!(
                "INFO",
                "{} {} {}",
                t!("commands.container_stopped"),
                container_info.name,
                status
            );
            return Ok(());
        }

        if Instant::now() >= deadline {
            log_bail!(
                "ERROR",
                "{}",
                t!(
                    "commands.stop_container_timeout",
                    "name" = container_info.name,
                    "timeout" = timeout_secs
                )
            );
        }

        sleep(Duration::from_secs(1)).await;
    }
}

fn is_running(status: &str) -> bool {
    matches!(status, "running" | "restarting")
}

fn print_container_table(containers: &[ContainerInfo]) {
    println!("\n{}:", t!("commands.available_containers"));
    println!(
        "{:<20} {:<24} {:<20}",
        t!("commands.container_name"),
        t!("commands.container_id"),
        t!("commands.container_status")
    );
    println!("{:-<64}", "");

    for container in containers {
        println!(
            "{:<20} {:<24} {:<20}",
            container.name, container.id, container.status
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::{ContainerInfo, DockerClient, MockDockerClientInterface};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn skips_stopping_when_not_running() -> Result<()> {
        DockerClient::init(10)?;
        let mut client = DockerClient::global()?;
        client
            .expect_get_container_status()
            .returning(|_| Ok("exited".to_string()));

        let container = ContainerInfo {
            id: "id".into(),
            name: "name".into(),
            status: "exited".into(),
        };

        ensure_container_stopped(&client, &container).await?;
        Ok(())
    }

    #[tokio::test]
    async fn stops_running_container_until_status_changes() -> Result<()> {
        let mut client = MockDockerClientInterface::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let status_counter = counter.clone();
        client
            .expect_get_container_status()
            .times(2)
            .returning(move |_| {
                let call = status_counter.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    Ok("running".to_string())
                } else {
                    Ok("exited".to_string())
                }
            });
        client
            .expect_stop_container()
            .times(1)
            .returning(|_| Ok(()));
        client.expect_get_stop_timeout_secs().returning(|| 2);

        let container = ContainerInfo {
            id: "id".into(),
            name: "name".into(),
            status: "running".into(),
        };

        ensure_container_stopped(&client, &container).await?;
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        Ok(())
    }
}
