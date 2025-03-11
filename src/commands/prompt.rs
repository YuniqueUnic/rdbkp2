use crate::{
    commands::{
        privileges::{has_admin_privileges, restart_with_admin_privileges},
        prompt_select,
    },
    docker::{ContainerInfo, DockerClientInterface, VolumeInfo},
    log_bail, log_println,
};

use anyhow::Result;
use dialoguer::{Confirm, MultiSelect, Select};
use tracing::{debug, info};

pub(super) fn require_admin_privileges_prompt() -> Result<()> {
    if has_admin_privileges() {
        return Ok(());
    }

    log_println!(
        "WARN",
        "❌ Please run as sudo user when restore the required container volume(s)."
    );
    let confirmed = Confirm::new()
        .with_prompt(t!("prompt.require_admin_privileges_prompt"))
        .default(true)
        .interact()?;

    if !confirmed {
        log_bail!("WARN", "{}", t!("prompt.restore_cancelled"));
    }

    restart_with_admin_privileges()?;

    log_bail!(
        "ERROR",
        "{}",
        t!("prompt.error_on_require_admin_privileges")
    )
}

pub(super) async fn select_container_prompt<T: DockerClientInterface>(
    client: &T,
) -> Result<ContainerInfo> {
    debug!("Getting container list for selection");
    let containers = client.list_containers().await?;
    let container_names: Vec<&String> = containers.iter().map(|c| &c.name).collect();

    debug!("Displaying container selection prompt");
    let selection = Select::new()
        .with_prompt(prompt_select(&format!(
            "{}",
            t!("prompt.select_container_prompt")
        )))
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
        .with_prompt(prompt_select(&format!(
            "{}",
            t!("prompt.select_containers_prompt")
        )))
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
    debug!(volumes = ?volumes, "Volumes to select from");

    let volume_names: Vec<String> = volumes
        .iter()
        .map(|v| format!("{} -> {}", v.source.display(), v.destination.display()))
        .collect();

    debug!("Displaying volume selection prompt");

    let selections = MultiSelect::new()
        .with_prompt(prompt_select(&format!(
            "{}",
            t!("prompt.select_volumes_prompt")
        )))
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
        .with_prompt(prompt_select(&format!(
            "{}",
            t!("prompt.select_volume_prompt")
        )))
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
