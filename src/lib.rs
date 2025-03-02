mod commands;
mod config;
mod docker;
mod utils;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::{io, path::PathBuf};
use tracing::{Level, info, instrument};
use tracing_subscriber::{EnvFilter, fmt};

#[allow(unused)]
pub(crate) const DOCKER_CMD: &str = "docker";

#[allow(unused)]
#[cfg(target_os = "macos")]
pub(crate) const DOCKER_COMPOSE_CMD: &str = "docker-compose";

#[allow(unused)]
#[cfg(not(target_os = "macos"))]
pub(crate) const DOCKER_COMPOSE_CMD: &str = "docker compose";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 是否在操作 (备份/恢复) 后重启容器
    #[arg(short, long)]
    restart: bool,

    /// 停止容器超时时间 (秒)
    #[arg(short, long, default_value = "30")]
    timeout: u64,

    /// 排除模式
    ///
    /// 备份时将排除包含这些模式的文件/目录
    #[arg(short, long, default_value = ".git,node_modules,target")]
    exclude: String,
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

        /// 需要备份的路径 (file/dir)
        ///
        /// 如果设置了该选项，则将只备份该路径下的数据
        /// 如果未设置该选项，则将备份容器内的所有 Volumes
        #[arg(short, long)]
        file: Option<String>,

        /// 备份文件输出路径
        #[arg(short, long)]
        #[arg(default_value = "./backup/")]
        output: Option<String>,

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

        /// 备份文件恢复输出路径
        #[arg(short, long)]
        output: Option<String>,

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

#[instrument(level = "INFO")]
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

const DEFAULT_CONFIG_FILE_PATH: &str = "./config.toml";

#[instrument(level = "INFO", fields(cfg_path = DEFAULT_CONFIG_FILE_PATH))]
pub fn load_config() -> Result<()> {
    let cfg_path = PathBuf::from(DEFAULT_CONFIG_FILE_PATH);

    if !cfg_path.exists() {
        let cfg = config::Config::default();
        cfg.save_to_file(cfg_path)?;
        config::Config::init(cfg)?;
    } else {
        config::Config::init_from_file(&cfg_path)?;
        config::Config::global()?.save_to_file(cfg_path)?;
    }

    Ok(())
}

#[instrument(level = "INFO")]
pub async fn run() -> Result<()> {
    info!("Starting Docker container backup tool");

    // 解析命令行参数
    let cli = Cli::parse();
    let timeout = cli.timeout;
    let restart = cli.restart;
    let exclude = cli.exclude;
    let exclude_patterns = exclude.split(',').collect::<Vec<&str>>();

    // 根据子命令执行相应的操作
    match cli.command {
        Commands::Backup {
            container,
            file,
            output,
            interactive,
        } => {
            info!(
                ?container,
                ?file,
                ?output,
                interactive,
                "Executing backup command"
            );
            commands::backup(
                container,
                file,
                output,
                restart,
                interactive,
                timeout,
                &exclude_patterns,
            )
            .await?;
        }
        Commands::Restore {
            container,
            file,
            output,
            interactive,
        } => {
            info!(
                ?container,
                ?file,
                ?output,
                interactive,
                "Executing restore command"
            );
            commands::restore(container, file, output, restart, interactive, timeout).await?;
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
                        clap_complete::aot::Bash,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::Fish => {
                    clap_complete::generate(
                        clap_complete::aot::Fish,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::Zsh => {
                    clap_complete::generate(
                        clap_complete::aot::Zsh,
                        &mut cmd,
                        &name,
                        &mut io::stdout(),
                    );
                }
                Shell::PowerShell => {
                    clap_complete::generate(
                        clap_complete::aot::PowerShell,
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
