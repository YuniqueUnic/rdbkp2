use super::privileges::has_admin_privileges;
use crate::{
    commands::prompt::require_admin_privileges_prompt,
    config::{self, Config},
    log_println,
};

use std::{fs, path::Path};

use anyhow::Result;

const SYMBOLINK_PATH: &str = "/usr/local/bin/rdbkp2";
pub(crate) fn create_symbollink() -> Result<()> {
    let path = Path::new(SYMBOLINK_PATH);
    if !has_admin_privileges() {
        require_admin_privileges_prompt()?;
    }

    let force = Config::global()?.yes;

    if force {}

    if path.is_symlink() {
        log_println!("INFO", "symbollink already exists");
        return Ok(());
    }

    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent path"))?;
        fs::create_dir_all(parent)?;
    }

    privilege::runas::Command::new("ln")
        .args(&["-s", "rdbkp2", SYMBOLINK_PATH])
        .run()?;

    log_println!("INFO", "create symbollink success on {}", SYMBOLINK_PATH);
    Ok(())
}

pub(crate) fn remove_symbollink() -> Result<()> {
    let path = Path::new(SYMBOLINK_PATH);
    if !has_admin_privileges() {
        require_admin_privileges_prompt()?;
    }

    let force = Config::global()?.yes;

    if force {}

    if !path.exists() {
        log_println!("INFO", "symbollink not exists");
        return Ok(());
    }

    if !path.is_symlink() {
        tracing::debug!("🤔 target is not symbollink");

        let ensure = dialoguer::Confirm::new()
            .with_prompt("🤔 target is not symbollink, continue to remove it?")
            .default(false)
            .interact()?;

        if !ensure {
            return Ok(());
        }
    }

    privilege::runas::Command::new("rm")
        .args(&[SYMBOLINK_PATH])
        .run()?;

    log_println!("INFO", "remove symbollink success at {}", SYMBOLINK_PATH);
    Ok(())
}
