use crate::DOCKER_COMPOSE_CMD;
use anyhow::Result;
use assert_fs::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use tokio::time::{Duration, sleep};
#[tokio::test]
async fn test_backup_restore_workflow() -> Result<()> {
    // 创建临时测试目录
    let temp = assert_fs::TempDir::new()?;
    let backup_dir = temp.child("backups");
    backup_dir.create_dir_all()?;

    // 启动 Docker 容器
    Command::new(DOCKER_COMPOSE_CMD)
        .args(&["-f", "docker/docker-compose.yaml", "up", "-d"])
        .status()?;

    // 等待服务启动
    sleep(Duration::from_secs(5)).await;

    // 生成测试数据
    for i in 1..=5 {
        let url = format!("http://localhost:8666/test?id={}", i);
        reqwest::get(&url).await?.text().await?;
        sleep(Duration::from_secs(1)).await;
    }

    // 验证日志文件存在并包含正确的条目数
    let log_file = PathBuf::from("docker/data/log.txt");
    assert!(log_file.exists());
    let log_content = std::fs::read_to_string(&log_file)?;
    assert!(log_content.lines().count() >= 5);

    // 执行备份
    Command::new(DOCKER_COMPOSE_CMD)
        .args(&["-f", "docker/docker-compose.yaml", "stop"])
        .status()?;

    let backup_path = backup_dir.path().to_str().unwrap();
    Command::new("cargo")
        .args(&["run", "--", "backup", "-c", "sim-server", "-o", backup_path])
        .status()?;

    // 修改配置
    let config_path = PathBuf::from("docker/config.yaml");
    let mut config_content = std::fs::read_to_string(&config_path)?;
    config_content =
        config_content.replace("Welcome to SimServer!", "Welcome to Updated SimServer!");
    config_content = config_content.replace("\"1.0\"", "\"2.0\"");
    std::fs::write(&config_path, config_content)?;

    // 清空数据目录
    std::fs::remove_dir_all("docker/data")?;
    std::fs::create_dir("docker/data")?;

    // 执行恢复
    Command::new("cargo")
        .args(&[
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
        .args(&["-f", "docker/docker-compose.yaml", "up", "-d"])
        .status()?;

    sleep(Duration::from_secs(5)).await;

    // 验证服务恢复
    let response = reqwest::get("http://localhost:8666").await?.text().await?;
    assert!(response.contains("Updated SimServer"));
    assert!(response.contains("2.0"));

    // 验证日志恢复
    let restored_log = std::fs::read_to_string("docker/data/log.txt")?;
    assert_eq!(log_content, restored_log);

    // 添加新数据
    for i in 6..=10 {
        let url = format!("http://localhost:8666/test?id={}", i);
        reqwest::get(&url).await?.text().await?;
        sleep(Duration::from_secs(1)).await;
    }

    // 验证新旧数据整合
    let final_log = std::fs::read_to_string("docker/data/log.txt")?;
    assert!(final_log.lines().count() >= 10);

    // 清理
    Command::new(DOCKER_COMPOSE_CMD)
        .args(&["-f", "docker/docker-compose.yaml", "down"])
        .status()?;

    Ok(())
}
