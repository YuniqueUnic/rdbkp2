use crate::{commands::privileges, config::Config, log_println};

use anyhow::{Context, Result};
use std::{fs, path::Path};

const SYMBOLINK_PATH: &str = "/usr/local/bin/rdbkp2";

/// 用户确认对话框
fn confirm_action(prompt: &str) -> Result<bool> {
    let ensure = dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .with_context(|| "用户输入错误")?;

    if !ensure {
        log_println!("INFO", "操作已取消");
    }
    Ok(ensure)
}

/// 检查路径状态并处理用户确认
fn check_path_status(path: &Path, force: bool, is_create: bool) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    if !force {
        let is_symlink = path.is_symlink();
        let action = if is_create { "创建" } else { "删除" };
        let prompt = if is_symlink {
            format!("🤔 已存在符号链接，是否继续{}？", action)
        } else {
            format!("🤔 目标不是符号链接，是否继续{}？", action)
        };

        tracing::debug!("🤔 路径状态检查：{}", prompt);
        return confirm_action(&prompt);
    }

    Ok(true)
}

pub(crate) fn create_symbollink() -> Result<()> {
    privileges::ensure_admin_privileges()?;
    let path = Path::new(SYMBOLINK_PATH);
    let force = Config::global()?.yes;

    // 检查路径状态
    if !check_path_status(path, force, true)? {
        return Ok(());
    }

    // 确保父目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("无法创建目录 {}", parent.display()))?;
    }

    let current_exe = std::env::current_exe()?;

    // 创建符号链接
    privilege::runas::Command::new("ln")
        .args(&[
            "-sf",
            &current_exe.to_string_lossy().to_string(),
            SYMBOLINK_PATH,
        ])
        .run()
        .with_context(|| format!("创建符号链接 {} 失败", SYMBOLINK_PATH))?;

    log_println!("INFO", "成功创建符号链接于 {}", SYMBOLINK_PATH);
    Ok(())
}

pub(crate) fn remove_symbollink() -> Result<()> {
    privileges::ensure_admin_privileges()?;
    let path = Path::new(SYMBOLINK_PATH);
    let force = Config::global()?.yes;

    if !path.exists() {
        log_println!("INFO", "符号链接不存在于 {}", SYMBOLINK_PATH);
        return Ok(());
    }

    // 检查路径状态
    if !check_path_status(path, force, false)? {
        return Ok(());
    }

    // 删除链接
    privilege::runas::Command::new("rm")
        .args(&["-f", SYMBOLINK_PATH])
        .run()
        .with_context(|| format!("删除 {} 失败", SYMBOLINK_PATH))?;

    log_println!("INFO", "成功删除符号链接 {}", SYMBOLINK_PATH);
    Ok(())
}
