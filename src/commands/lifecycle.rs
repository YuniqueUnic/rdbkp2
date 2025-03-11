use crate::{commands::symbollink, log_println};
use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;

const CRATE_NAME: &str = "rdbkp2";
const CARGO_IO_API: &str = "https://crates.io/api/v1/crates/";

#[derive(Deserialize)]
struct CrateResponse {
    versions: Vec<CrateVersion>,
}

#[derive(Deserialize)]
struct CrateVersion {
    num: String,
    yanked: bool,
}

/// 检查新版本
pub async fn check_update() -> Result<()> {
    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))?;

    // 获取 crates.io 上的版本信息
    let client = reqwest::Client::new();
    let url = format!("{}{}", CARGO_IO_API, CRATE_NAME);
    let response = client
        .get(&url)
        .header("User-Agent", format!("{}/{}", CRATE_NAME, current_version))
        .send()
        .await
        .with_context(|| t!("lifecycle.can_not_connect_to_crates_io"))?;

    let crate_info: CrateResponse = response
        .json()
        .await
        .with_context(|| t!("lifecycle.can_not_parse_version_info"))?;

    // 找到最新的未被撤回的版本
    let latest_version = crate_info
        .versions
        .iter()
        .filter(|v| !v.yanked)
        .next()
        .ok_or_else(|| anyhow::anyhow!(t!("lifecycle.no_available_version")))?;

    let latest_version = Version::parse(&latest_version.num)?;

    if latest_version > current_version {
        log_println!(
            "INFO",
            "{}",
            format!(
                "{} ({})",
                t!(
                    "lifecycle.new_version_found",
                    "latest_version" = latest_version
                ),
                t!(
                    "lifecycle.current_version",
                    "current_version" = current_version
                )
            )
        );
        log_println!(
            "INFO",
            "{}",
            format!(
                "{} (cargo install {} --force)",
                t!("lifecycle.update_command"),
                CRATE_NAME
            )
        );
    } else {
        log_println!(
            "INFO",
            "{}",
            t!(
                "lifecycle.current_version",
                "current_version" = current_version
            )
        );
    }

    Ok(())
}

/// 完全卸载，包括删除符号链接
pub async fn uninstall() -> Result<()> {
    // 1. 删除符号链接
    if let Err(e) = symbollink::remove_symbollink() {
        log_println!(
            "WARN",
            "{}",
            t!("lifecycle.remove_symbollink_failed", "error" = e)
        );
    }

    // 2. 提示用户如何完成卸载
    log_println!(
        "INFO",
        "{} (cargo uninstall {})",
        t!("lifecycle.uninstall_command"),
        CRATE_NAME
    );

    Ok(())
}
