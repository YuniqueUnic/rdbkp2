use crate::{
    commands::privileges::{has_admin_privileges, restart_with_admin_privileges},
    docker::{ContainerInfo, DockerClientInterface, VolumeInfo},
    log_bail, log_println, prompt_select,
};

use anyhow::Result;
use dialoguer::{Confirm, MultiSelect, Select};
use tracing::{debug, info};

pub(super) fn require_admin_privileges_prompt() -> Result<()> {
    if !has_admin_privileges() {
        log_println!(
            "WARN",
            "❌ Please run as sudo user when restore the required container volume(s)."
        );
        let confirmed = Confirm::new()
            .with_prompt("👌 Do you want to restart with sudo?")
            .default(true)
            .interact()?;

        if !confirmed {
            log_println!("INFO", "⛔ Restore cancelled");
            return Ok(());
        }

        restart_with_admin_privileges()?;
    }

    log_bail!("ERROR", "👿 error on requiring admin privileges")
}

pub(super) async fn select_container_prompt<T: DockerClientInterface>(
    client: &T,
) -> Result<ContainerInfo> {
    debug!("Getting container list for selection");
    let containers = client.list_containers().await?;
    let container_names: Vec<&String> = containers.iter().map(|c| &c.name).collect();

    debug!("Displaying container selection prompt");
    let selection = Select::new()
        .with_prompt(prompt_select!("🫙 Select one container"))
        .items(&container_names)
        .default(0)
        .interact()?;

    let selected = containers[selection].clone();
    info!(
        container_name = ?selected.name,
        container_id = ?selected.id,
        "Container selected"
    );
    Ok(selected)
}

#[allow(dead_code)]
pub(super) async fn select_containers_prompt<T: DockerClientInterface>(
    client: &T,
) -> Result<Vec<ContainerInfo>> {
    debug!("Getting container list for selection");
    let containers = client.list_containers().await?;
    let container_names: Vec<&String> = containers.iter().map(|c| &c.name).collect();

    debug!("Displaying container multi-selection prompt");
    let selections = MultiSelect::new()
        .with_prompt(prompt_select!("🫙 Select one or more containers"))
        .items(&container_names)
        .defaults(&[true])
        .interact()?;

    let selected: Vec<ContainerInfo> = selections.iter().map(|i| containers[*i].clone()).collect();
    info!(
        selected_containers = ?selected.iter().map(|c| &c.name).collect::<Vec<_>>(),
        "Containers selected"
    );
    Ok(selected)
}

pub(super) fn select_volumes_prompt(volumes: &[VolumeInfo]) -> Result<Vec<VolumeInfo>> {
    debug!(volume_count = volumes.len(), "Preparing volume selection");
    let volume_names: Vec<String> = volumes
        .iter()
        .map(|v| format!("{} -> {}", v.source.display(), v.destination.display()))
        .collect();

    debug!("Displaying volume selection prompt");

    let selections = MultiSelect::new()
        .with_prompt(prompt_select!(
            "   host ------------> container\n\
            📃 Select one or more volume(s)"
        ))
        .items(&volume_names)
        .defaults(&[true])
        .interact()?;

    let selected: Vec<VolumeInfo> = selections.iter().map(|i| volumes[*i].clone()).collect();
    info!(
        selected_volumes = ?selected.iter().map(|v| &v.name).collect::<Vec<_>>(),
        "Volumes selected"
    );
    Ok(selected)
}

#[allow(dead_code)]
pub(super) fn select_volume_prompt(volumes: &[VolumeInfo]) -> Result<VolumeInfo> {
    let volume_names: Vec<String> = volumes
        .iter()
        .map(|v| format!("{} -> {}", v.source.display(), v.destination.display()))
        .collect();

    let selection = Select::new()
        .with_prompt(prompt_select!(
            "   host -> container\n\
            📃 Select one volume"
        ))
        .items(&volume_names)
        .default(0)
        .interact()?;

    let selected = volumes[selection].clone();
    info!(
        selected_volume = ?selected.name,
        "Volume selected"
    );
    Ok(selected)
}
