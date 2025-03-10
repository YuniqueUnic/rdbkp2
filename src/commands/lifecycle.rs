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
        .with_context(|| "无法连接到 crates.io")?;

    let crate_info: CrateResponse = response.json().await.with_context(|| "无法解析版本信息")?;

    // 找到最新的未被撤回的版本
    let latest_version = crate_info
        .versions
        .iter()
        .filter(|v| !v.yanked)
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到可用版本"))?;

    let latest_version = Version::parse(&latest_version.num)?;

    if latest_version > current_version {
        log_println!(
            "INFO",
            "发现新版本：{} (当前版本：{})",
            latest_version,
            current_version
        );
        log_println!(
            "INFO",
            "运行以下命令更新：\n  cargo install {} --force",
            CRATE_NAME
        );
    } else {
        log_println!("INFO", "当前已是最新版本：{}", current_version);
    }

    Ok(())
}

/// 完全卸载，包括删除符号链接
pub async fn uninstall() -> Result<()> {
    // 1. 删除符号链接
    if let Err(e) = symbollink::remove_symbollink() {
        log_println!("WARN", "删除符号链接失败：{}", e);
    }

    // 2. 提示用户如何完成卸载
    log_println!(
        "INFO",
        "符号链接已删除，请运行以下命令完成卸载：\n  cargo uninstall {}",
        CRATE_NAME
    );

    Ok(())
}
