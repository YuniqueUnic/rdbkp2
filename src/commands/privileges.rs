use anyhow::Result;

#[cfg(target_os = "windows")]
use runas::Command as RunasCommand;

use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;

use super::prompt;

// 检查是否有管理员权限
pub(super) fn has_admin_privileges() -> bool {
    tracing::debug!("{}", t!("privileges.has_admin_privileges"));
    privilege::user::privileged()
}

pub(super) fn ensure_admin_privileges() -> Result<()> {
    if !has_admin_privileges() {
        prompt::require_admin_privileges_prompt()?;
    }
    Ok(())
}

// 以管理员权限重启程序
#[allow(unreachable_code)]
pub(super) fn restart_with_admin_privileges() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(debug_assertions)]
    {
        tracing::info!("restart with admin privileges: {}", args.join(" "));
    }

    let mut cmd = privilege::runas::Command::new(current_exe);

    #[cfg(target_os = "windows")]
    {
        cmd.gui(true);
    }

    cmd.args(&args)
        .force_prompt(!has_admin_privileges())
        .run()
        .map_err(|e| {
            anyhow::anyhow!(
                "{}",
                t!("privileges.restart_with_admin_privileges", "error" = e)
            )
        })?;

    std::process::exit(0);

    Ok(())
}

/// 使用特权方式复制文件或目录
pub(super) fn privileged_copy(from: &Path, to: &Path) -> Result<()> {
    // 检查源路径是文件还是目录
    let is_dir = std::fs::metadata(from)?.is_dir();

    #[cfg(target_os = "windows")]
    {
        // Windows 下已经有管理员权限，直接复制
        if is_dir {
            let copy_options = fs_extra::dir::CopyOptions {
                overwrite: true,
                skip_exist: false,
                content_only: true,
                ..Default::default()
            };
            fs_extra::dir::copy(from, to, &copy_options)
                .map_err(|e| anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e)))?;
        } else {
            let copy_options = fs_extra::file::CopyOptions {
                overwrite: true,
                skip_exist: false,
                ..Default::default()
            };
            fs_extra::file::copy(from, to, &copy_options)
                .map_err(|e| anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e)))?;
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // 在 Linux/macOS 下，如果已经是 root，直接复制
        if has_admin_privileges() {
            if is_dir {
                let copy_options = fs_extra::dir::CopyOptions {
                    overwrite: true,
                    skip_exist: false,
                    content_only: true,
                    ..Default::default()
                };
                fs_extra::dir::copy(from, to, &copy_options).map_err(|e| {
                    anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e))
                })?;
            } else {
                let copy_options = fs_extra::file::CopyOptions {
                    overwrite: true,
                    skip_exist: false,
                    ..Default::default()
                };
                fs_extra::file::copy(from, to, &copy_options).map_err(|e| {
                    anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e))
                })?;
            }
        } else {
            // 否则使用 sudo 命令复制
            let mut cmd = Command::new("sudo");
            cmd.arg("cp");

            if is_dir {
                cmd.arg("-r");
            }

            // 确保目标目录存在（对于文件复制）
            if !is_dir {
                if let Some(parent) = to.parent() {
                    if !parent.exists() {
                        // 创建父目录
                        let mkdir_status = Command::new("sudo")
                            .arg("mkdir")
                            .arg("-p")
                            .arg(parent)
                            .status()
                            .map_err(|e| {
                                anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e))
                            })?;

                        if !mkdir_status.success() {
                            return Err(anyhow::anyhow!(
                                "{}",
                                t!("privileges.copy_failed_parent_dir", "error" = "sudo mkdir")
                            ));
                        }
                    }
                }
            }

            let status =
                cmd.arg(from).arg(to).status().map_err(|e| {
                    anyhow::anyhow!("{}", t!("privileges.copy_failed", "error" = e))
                })?;

            if !status.success() {
                return Err(anyhow::anyhow!(
                    "{}",
                    t!("privileges.copy_failed", "error" = "sudo cp")
                ));
            }

            // 如果是目录，确保权限正确
            if is_dir {
                let chmod_status = Command::new("sudo")
                    .arg("chmod")
                    .arg("-R")
                    .arg("755") // 或者使用更合适的权限
                    .arg(to)
                    .status()
                    .map_err(|e| {
                        anyhow::anyhow!("{}", t!("privileges.set_permissions_failed", "error" = e))
                    })?;

                if !chmod_status.success() {
                    return Err(anyhow::anyhow!(
                        "{}",
                        t!("privileges.set_permissions_failed", "error" = "chmod")
                    ));
                }
            }
        }
    }

    Ok(())
}
