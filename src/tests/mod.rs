use crate::{DOCKER_CMD, DOCKER_COMPOSE_CMD};
use anyhow::Result;
use assert_fs::{TempDir, prelude::*};
use std::process::Command;
use std::{env, path::PathBuf};
use tokio::time::{Duration, sleep};

pub(crate) fn get_docker_compose_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Failed to get manifest directory");
    PathBuf::from(manifest_dir).join("docker")
}

// 检查命令是否存在
pub(crate) fn check_docker_compose() -> Result<()> {
    let output = Command::new(DOCKER_CMD)
        .arg("--version")
        .output()
        .map_err(|_| {
            anyhow::anyhow!("docker compose command not found. Please install Docker Compose.")
        })?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to run docker compose command"));
    }

    Ok(())
}

#[ignore = "This test needs a really docker environment, manual test is recommended"]
#[tokio::test]
async fn test_backup_restore_workflow() -> Result<()> {
    // 创建临时测试目录
    let temp = TempDir::new()?;
    let backup_dir = temp.child("backups");
    backup_dir.create_dir_all()?;

    let docker_dir = get_docker_compose_path();

    // 启动 Docker 容器
    let status = Command::new(DOCKER_COMPOSE_CMD)
        .current_dir(&docker_dir)
        .args(["-f", "docker-compose.yaml", "up", "-d"])
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Failed to start Docker containers. Please check if the docker-compose.yaml file exists and is correctly configured."
        ));
    }

    // 等待服务启动
    sleep(Duration::from_secs(5)).await;

    // 生成测试数据
    for i in 1..=5 {
        let url = format!("http://localhost:8666/test?id={}", i);
        reqwest::get(&url).await?.text().await?;
        sleep(Duration::from_secs(1)).await;
    }

    // 验证日志文件存在并包含正确的条目数
    let data_dir = PathBuf::from(&docker_dir).join("data");
    let log_file = PathBuf::from(&data_dir).join("log.txt");
    assert!(log_file.exists());
    let log_content = std::fs::read_to_string(&log_file)?;
    assert!(log_content.lines().count() >= 5);

    // 执行备份
    Command::new(DOCKER_COMPOSE_CMD)
        .current_dir(&docker_dir)
        .args(["-f", "docker-compose.yaml", "stop"])
        .status()?;

    let backup_path = backup_dir.path().to_str().unwrap();
    Command::new("cargo")
        .args(["run", "--", "backup", "-c", "sim-server", "-o", backup_path])
        .status()?;

    // 修改配置
    let config_path = PathBuf::from(&docker_dir).join("config.ini");
    let mut config_content = std::fs::read_to_string(&config_path)?;
    config_content =
        config_content.replace("Welcome to SimServer!", "Welcome to Updated SimServer!");
    config_content = config_content.replace("\"1.0\"", "\"2.0\"");
    std::fs::write(&config_path, config_content)?;

    // 清空数据目录
    // let data_dir = PathBuf::from(&docker_dir).join("data");
    std::fs::remove_dir_all(&data_dir)?;
    std::fs::create_dir(&data_dir)?;

    // 执行恢复
    Command::new("cargo")
        .args([
            "run",
            "--",
            "restore",
            "-c",
            "sim-server",
            "-i",
            backup_path,
        ])
        .status()?;

    // 重启服务
    Command::new(DOCKER_COMPOSE_CMD)
        .current_dir(&docker_dir)
        .args(["-f", "docker-compose.yaml", "up", "-d"])
        .status()?;

    sleep(Duration::from_secs(5)).await;

    // 验证服务恢复
    let response = reqwest::get("http://localhost:8666").await?.text().await?;
    assert!(response.contains("Updated SimServer"));
    assert!(response.contains("2.0"));

    // 验证日志恢复
    // let log_file = PathBuf::from(&data_dir).join("log.txt");
    let restored_log = std::fs::read_to_string(&log_file)?;
    assert_eq!(log_content, restored_log);

    // 添加新数据
    for i in 6..=10 {
        let url = format!("http://localhost:8666/test?id={}", i);
        reqwest::get(&url).await?.text().await?;
        sleep(Duration::from_secs(1)).await;
    }

    // 验证新旧数据整合
    let final_log = std::fs::read_to_string(&log_file)?;
    assert!(final_log.lines().count() >= 10);

    // 清理
    Command::new(DOCKER_COMPOSE_CMD)
        .current_dir(&docker_dir)
        .args(["-f", "docker-compose.yaml", "down"])
        .status()?;

    Ok(())
}
