use anyhow::Result;

#[cfg(target_os = "windows")]
use runas::Command as RunasCommand;

use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;

// 检查是否有管理员权限
pub(super) fn has_admin_privileges() -> bool {
    #[cfg(target_os = "windows")]
    {
        use winapi::um::securitybaseapi::IsUserAnAdmin;
        unsafe { IsUserAnAdmin() != 0 }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|output| {
                let uid = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(1000);
                uid == 0
            })
            .unwrap_or(false)
    }
}

// 以管理员权限重启程序
#[allow(unreachable_code)]
pub(super) fn restart_with_admin_privileges() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        RunasCommand::new(current_exe)
            .args(&args)
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to restart with admin privileges: {}", e))?;

        std::process::exit(0);
    }

    #[cfg(target_os = "linux")]
    {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        Command::new("pkexec")
            .arg(current_exe)
            .args(args)
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to restart with admin privileges: {}", e))?;

        std::process::exit(0);
    }

    #[cfg(target_os = "macos")]
    {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "do shell script \"'{}' {}\" with administrator privileges",
                current_exe.to_string_lossy(),
                args.join(" ")
            ))
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to restart with admin privileges: {}", e))?;

        std::process::exit(0);
    }

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
            dir::copy(from, to, &copy_options)
                .map_err(|e| anyhow::anyhow!("Failed to copy directory data: {}", e))?;
        } else {
            let copy_options = fs_extra::file::CopyOptions {
                overwrite: true,
                skip_exist: false,
                ..Default::default()
            };
            file::copy(from, to, &copy_options)
                .map_err(|e| anyhow::anyhow!("Failed to copy file data: {}", e))?;
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
                fs_extra::dir::copy(from, to, &copy_options)
                    .map_err(|e| anyhow::anyhow!("Failed to copy directory data: {}", e))?;
            } else {
                let copy_options = fs_extra::file::CopyOptions {
                    overwrite: true,
                    skip_exist: false,
                    ..Default::default()
                };
                fs_extra::file::copy(from, to, &copy_options)
                    .map_err(|e| anyhow::anyhow!("Failed to copy file data: {}", e))?;
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
                                anyhow::anyhow!("Failed to create parent directory: {}", e)
                            })?;

                        if !mkdir_status.success() {
                            return Err(anyhow::anyhow!(
                                "Failed to create parent directory, exit code: {:?}",
                                mkdir_status.code()
                            ));
                        }
                    }
                }
            }

            let status = cmd
                .arg(from)
                .arg(to)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to execute sudo cp: {}", e))?;

            if !status.success() {
                return Err(anyhow::anyhow!(
                    "Failed to copy with sudo, exit code: {:?}",
                    status.code()
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
                    .map_err(|e| anyhow::anyhow!("Failed to set permissions: {}", e))?;

                if !chmod_status.success() {
                    return Err(anyhow::anyhow!(
                        "Failed to set permissions, exit code: {:?}",
                        chmod_status.code()
                    ));
                }
            }
        }
    }

    Ok(())
}
