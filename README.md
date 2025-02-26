# Docker Container Data Backup Tool

一个用于备份和恢复 Docker 容器数据的命令行工具。

## 功能特点

- 支持备份和恢复 Docker 容器的数据卷
- 支持命令行参数和交互式操作
- 使用 XZ 压缩算法进行高效压缩
- 支持自定义配置文件

## 安装

确保你的系统已安装 Rust 工具链，然后执行：

```bash
cargo install --path .
```

## 使用方法

### 列出可用的容器

```bash
rdbkp2 list
```

### 备份容器数据

交互式模式：
```bash
rdbkp2 backup -i
```

命令行模式：
```bash
rdbkp2 backup -c container_name -o /path/to/backup/dir
```

### 恢复容器数据

交互式模式：
```bash
rdbkp2 restore -i
```

命令行模式：
```bash
rdbkp2 restore -c container_name -i /path/to/backup/file
```

## 配置文件

默认配置文件位于 `config.toml`，可以自定义以下设置：

```toml
# 备份文件的默认输出目录
backup_dir = "./backups"

# Docker 相关配置
[docker]
# Docker daemon 的地址
host = "unix:///var/run/docker.sock"
# 是否使用 TLS
tls = false
# 证书路径 (如果使用 TLS)
# cert_path = "/path/to/cert"
```

## 命令行参数

### 备份命令 (backup)

- `-c, --container`: 容器名称或 ID
- `-o, --output`: 输出目录
- `-i, --interactive`: 使用交互式模式

### 恢复命令 (restore)

- `-c, --container`: 容器名称或 ID
- `-i, --input`: 备份文件路径
- `-i, --interactive`: 使用交互式模式

### 列表命令 (list)

无参数，显示所有可用的容器。

## 注意事项

1. 确保有足够的磁盘空间用于备份
2. 建议在恢复数据之前先备份当前数据
3. 需要有访问 Docker daemon 的权限
4. Windows 用户需要确保 Docker Desktop 已启动 