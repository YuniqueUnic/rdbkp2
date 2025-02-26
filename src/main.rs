mod commands;
mod config;
mod docker;
mod utils;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 备份 Docker 容器数据
    Backup {
        /// 容器名称或ID
        #[arg(short, long)]
        container: Option<String>,

        /// 输出目录
        #[arg(short, long)]
        output: Option<String>,

        /// 是否使用交互模式
        #[arg(short, long)]
        interactive: bool,
    },

    /// 恢复 Docker 容器数据
    Restore {
        /// 容器名称或ID
        #[arg(short, long)]
        container: Option<String>,

        /// 备份文件路径
        #[arg(short, long)]
        input: Option<String>,

        /// 是否使用交互模式
        #[arg(short, long)]
        interactive: bool,
    },

    /// 列出可用的 Docker 容器
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
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

    info!("Starting Docker container backup tool");

    // 解析命令行参数
    let cli = Cli::parse();

    // 根据子命令执行相应的操作
    match cli.command {
        Commands::Backup {
            container,
            output,
            interactive,
        } => {
            info!(?container, ?output, interactive, "Executing backup command");
            commands::backup(container, output, interactive).await?;
        }
        Commands::Restore {
            container,
            input,
            interactive,
        } => {
            info!(?container, ?input, interactive, "Executing restore command");
            commands::restore(container, input, interactive).await?;
        }
        Commands::List => {
            info!("Executing list command");
            commands::list_containers().await?;
        }
    }

    info!("Operation completed successfully");
    Ok(())
}
