mod commands;
mod config;
mod docker;
mod utils;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::io;
use tracing::{Level, info};
use tracing_subscriber::{EnvFilter, fmt};

pub(crate) const DOCKER_CMD: &str = "docker";

#[cfg(target_os = "macos")]
pub(crate) const DOCKER_COMPOSE_CMD: &str = "docker-compose";

#[cfg(not(target_os = "macos"))]
pub(crate) const DOCKER_COMPOSE_CMD: &str = "docker compose";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 停止容器超时时间
    #[arg(short, long, default_value = "30")]
    timeout: u64,
}

#[derive(Clone, ValueEnum, Debug)]
enum Shell {
    Bash,
    Fish,
    Zsh,
    PowerShell,
}

#[derive(Subcommand)]
enum Commands {
    /// 备份 Docker 容器数据
    ///
    /// 备份时将会进行的操作：
    /// 1. 检查容器是否存在
    /// 2. 检查容器是否正在运行，如果正在运行，则先停止容器
    /// 3. 检查容器是否存在挂载卷
    /// 4. 备份挂载卷
    /// 5. 压缩挂载卷
    /// 6. 将压缩后的挂载卷移动到输出目录
    /// 7. 如果设置了 --restart 选项，则重启容器
    Backup {
        /// 容器名称或 ID
        #[arg(short, long)]
        container: Option<String>,

        /// 数据路径
        ///
        /// 如果设置了该选项，则将只备份该路径下的数据
        /// 如果未设置该选项，则将备份容器内的所有 Volumes
        #[arg(short, long)]
        data_path: Option<String>,

        /// 输出目录
        #[arg(short, long)]
        #[arg(default_value = "./backup/")]
        output: Option<String>,

        /// 是否在备份后重启容器
        #[arg(short, long)]
        restart: bool,

        /// 是否使用交互模式
        #[arg(short, long)]
        interactive: bool,
    },

    /// 恢复 Docker 容器数据
    ///
    /// 恢复时将会进行的操作：
    /// 1. 检查容器是否存在
    /// 2. 检查容器是否正在运行，如果正在运行，则先停止容器
    /// 3. 检查容器是否存在挂载卷
    /// 4. 恢复挂载卷
    /// 5. 如果设置了 --restart 选项，则重启容器
    Restore {
        /// 容器名称或 ID
        #[arg(short, long)]
        container: Option<String>,

        /// 备份文件路径
        #[arg(short, long)]
        file: Option<String>,

        /// 是否在恢复后重启容器
        #[arg(short, long)]
        restart: bool,

        /// 是否使用交互模式
        #[arg(short, long)]
        interactive: bool,
    },

    /// 列出可用的 Docker 容器
    List,

    /// 生成命令行补全脚本
    Completions {
        /// Shell 类型
        #[arg(value_enum)]
        shell: Shell,
    },
}

pub fn init_log() -> Result<()> {
    // 初始化日志
    fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .with_level(true)
        .init();
    Ok(())
}

pub async fn run() -> Result<()> {
    info!("Starting Docker container backup tool");

    // 解析命令行参数
    let cli = Cli::parse();
    let timeout = cli.timeout;

    // 根据子命令执行相应的操作
    match cli.command {
        Commands::Backup {
            container,
            data_path,
            output,
            restart,
            interactive,
        } => {
            info!(
                ?container,
                ?data_path,
                ?output,
                interactive,
                "Executing backup command"
            );
            commands::backup(container, data_path, output, restart, interactive, timeout).await?;
        }
        Commands::Restore {
            container,
            file: input,
            restart,
            interactive,
        } => {
            info!(?container, ?input, interactive, "Executing restore command");
            commands::restore(container, input, restart, interactive, timeout).await?;
        }
        Commands::List => {
            info!("Executing list command");
            commands::list_containers().await?;
        }
        Commands::Completions { shell } => {
            info!(?shell, "Generating shell completions");
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            match shell {
                Shell::Bash => {
                    clap_complete::generate(
                        clap_complete::shells::Bash,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::Fish => {
                    clap_complete::generate(
                        clap_complete::shells::Fish,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::Zsh => {
                    clap_complete::generate(
                        clap_complete::shells::Zsh,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::PowerShell => {
                    clap_complete::generate(
                        clap_complete::shells::PowerShell,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
            }
        }
    }

    info!("Operation completed successfully");
    Ok(())
}

#[cfg(test)]
pub(crate) fn init_test_log() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::DEBUG.into())
                .from_env_lossy(),
        )
        .with_test_writer()
        .try_init();
}
