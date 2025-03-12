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
        .with_context(|| t!("symbollink.user_input_error"))?;

    if !ensure {
        log_println!("INFO", "{}", t!("symbollink.action_cancelled"));
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
        let action = if is_create {
            t!("symbollink.create")
        } else {
            t!("symbollink.remove")
        };
        let prompt = if is_symlink {
            format!(
                "{}，{} {}？",
                t!("symbollink.already_exists"),
                t!("symbollink.confirm_action"),
                action
            )
        } else {
            format!(
                "{}，{} {}？",
                t!("symbollink.not_symlink"),
                t!("symbollink.confirm_action"),
                action
            )
        };

        tracing::debug!("{}：{}", t!("symbollink.path_status_check"), prompt);
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
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "{}",
                t!(
                    "symbollink.failed_to_create_directory",
                    "directory" = parent.display()
                )
            )
        })?;
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
        .with_context(|| {
            format!(
                "{}",
                t!(
                    "symbollink.failed_to_create_symbollink",
                    "path" = SYMBOLINK_PATH
                )
            )
        })?;

    log_println!(
        "INFO",
        "{}",
        t!(
            "symbollink.success_create_symbollink",
            "path" = SYMBOLINK_PATH
        )
    );
    Ok(())
}

pub(crate) fn remove_symbollink() -> Result<()> {
    privileges::ensure_admin_privileges()?;
    let path = Path::new(SYMBOLINK_PATH);
    let force = Config::global()?.yes;

    if !path.exists() {
        log_println!(
            "INFO",
            "{}",
            t!("symbollink.symbollink_not_exists", "path" = SYMBOLINK_PATH)
        );
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
        .with_context(|| {
            format!(
                "{}",
                t!(
                    "symbollink.failed_to_remove_symbollink",
                    "path" = SYMBOLINK_PATH
                )
            )
        })?;

    log_println!(
        "INFO",
        "{}",
        t!(
            "symbollink.success_remove_symbollink",
            "path" = SYMBOLINK_PATH
        )
    );
    Ok(())
}
