mod commands;
// #[deprecated(since = "1.0.0", note = "no need to load config file")]
mod config;
mod docker;
mod utils;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::io;
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

    /// 是否使用交互模式 [default: true]
    #[arg(global = true, short, long, default_value = "true")]
    interactive: bool,

    /// 是否在操作 (备份/恢复) 后重启容器 [default: false]
    #[arg(global = true, short, long, default_value = "false")]
    restart: bool,

    /// 停止容器超时时间 (秒)
    #[arg(global = true, short, long, default_value = "30")]
    timeout: u64,

    /// 排除模式：备份时将排除包含这些模式的文件/目录
    #[arg(global = true, short, long, default_value = ".git,node_modules,target")]
    exclude: String,

    /// 是否自动确认 [default: false]
    #[arg(global = true, short, long, default_value = "false")]
    yes: bool,

    /// 是否显示详细日志 [default: false]
    #[arg(global = true, short, long, default_value = "false")]
    verbose: bool,
}

#[derive(Clone, ValueEnum, Debug)]
enum Shell {
    Bash,
    Fish,
    Zsh,
    PowerShell,
}

impl Into<clap_complete::aot::Shell> for Shell {
    fn into(self) -> clap_complete::aot::Shell {
        match self {
            Shell::Bash => clap_complete::aot::Shell::Bash,
            Shell::Fish => clap_complete::aot::Shell::Fish,
            Shell::Zsh => clap_complete::aot::Shell::Zsh,
            Shell::PowerShell => clap_complete::aot::Shell::PowerShell,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// 备份 Docker 容器数据
    ///
    /// 备份时将会进行的操作：
    /// 1. 检查容器是否存在
    /// 2. 检查容器是否正在运行，如果正在运行，则先停止容器
    /// 3. 检查容器是否存在挂载卷
    /// 4. 压缩备份挂载卷到输出目录
    /// 5. 如果设置了 --restart 选项，则重启容器
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
    },

    /// 恢复 Docker 容器数据
    ///
    /// 恢复时将会进行的操作：
    /// 1. 检查容器是否存在
    /// 2. 检查容器是否正在运行，如果正在运行，则先停止容器
    /// 3. 恢复挂载卷到指定路径 (如果未指定，则恢复到容器工作目录)
    /// 4. 如果设置了 --restart 选项，则重启容器
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
fn init_config(
    timeout_secs: u64,
    interactive: bool,
    restart: bool,
    verbose: bool,
    yes: bool,
    exclude: String,
) -> Result<()> {
    let mut cfg = config::Config::default();
    cfg.timeout_secs = timeout_secs;
    cfg.interactive = interactive;
    cfg.restart = restart;
    cfg.verbose = verbose;
    cfg.yes = yes;
    cfg.exclude = exclude;
    config::Config::init(cfg)?;
    Ok(())
}

#[instrument(level = "INFO")]
pub fn init_log(log_level: Level) -> Result<()> {
    // 初始化日志
    let mut log_fmt = fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(log_level.into())
                .from_env_lossy(),
        )
        .with_level(true);

    #[cfg(debug_assertions)]
    {
        log_fmt = log_fmt
            .with_target(true)
            .with_thread_ids(true)
            .with_line_number(true)
            .with_file(true);
    }

    log_fmt.init();
    Ok(())
}

#[instrument(level = "INFO")]
fn init_docker_client(timeout_secs: u64) -> Result<()> {
    docker::DockerClient::init(timeout_secs)?;
    Ok(())
}

#[instrument(level = "INFO")]
pub async fn run() -> Result<()> {
    info!("Starting Docker container backup tool");

    // 解析命令行参数
    let cli = Cli::parse();
    let interactive = cli.interactive;
    let timeout = cli.timeout;
    let restart = cli.restart;
    let exclude = cli.exclude;
    let yes = cli.yes;
    let verbose = cli.verbose;

    // 初始化全局 runtime 配置
    init_config(timeout, interactive, restart, verbose, yes, exclude)?;

    // 设置日志级别，初始化全局日志
    let log_level = if verbose { Level::DEBUG } else { Level::ERROR };
    init_log(log_level)?;

    // 初始化全局 docker client
    init_docker_client(timeout)?;

    // 根据子命令执行相应的操作
    do_action(cli.command).await?;

    info!("Operation completed successfully");
    Ok(())
}

async fn do_action(action: Commands) -> Result<()> {
    match action {
        Commands::Backup {
            container,
            file,
            output,
        } => {
            info!(?container, ?file, ?output, "Executing backup command");
            commands::backup(container, file, output).await?;
        }
        Commands::Restore {
            container,
            file,
            output,
        } => {
            info!(?container, ?file, ?output, "Executing restore command");
            commands::restore(container, file, output).await?;
        }
        Commands::List => {
            info!("Executing list command");
            commands::list_containers().await?;
        }
        Commands::Completions { shell } => {
            info!(?shell, "Generating shell completions");
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(
                clap_complete::aot::Shell::from(shell.into()),
                &mut cmd,
                &name,
                &mut io::stdout(),
            );
        }
    }
    Ok(())
}
