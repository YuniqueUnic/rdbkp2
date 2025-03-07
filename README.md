# Docker Container Data Backup Tool

[EN README.md](./README_EN.md)

一个用于备份和恢复 Docker 容器数据的命令行工具。

## 功能特点

- 支持备份和恢复 Docker 容器的数据卷
- 支持命令行参数和交互式操作
- 使用 XZ 压缩算法进行高效压缩
- 支持自定义配置文件
- 支持命令行补全（Bash/Zsh/Fish/PowerShell）

## 安装

确保你的系统已安装 Rust 工具链，然后执行：

```bash
cargo install rdbkp2                             # isntall rdbkp2
sudo ln -s $(where rdbkp2) /usr/local/bin/rdbkp2 # symbol link rdbkp2 for sudo execuation
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
rdbkp2 restore -c container_name -f /path/to/backup/file
```

### 命令行补全

生成命令行补全脚本，支持多种 shell：

```bash
# 生成 Bash 补全脚本
rdbkp2 completions bash > ~/.local/share/bash-completion/completions/rdbkp2

# 生成 Zsh 补全脚本
rdbkp2 completions zsh > ~/.zsh/_rdbkp2

# 生成 Fish 补全脚本
rdbkp2 completions fish > ~/.config/fish/completions/rdbkp2.fish

# 生成 PowerShell 补全脚本
# Windows PowerShell
mkdir -p $PROFILE\..\Completions
rdbkp2 completions powershell > $PROFILE\..\Completions\rdbkp2.ps1
```

#### 启用补全功能

##### Bash

将以下内容添加到 `~/.bashrc` 或 `~/.bash_profile`：

```bash
source ~/.local/share/bash-completion/completions/rdbkp2
```

##### Zsh

将补全脚本放置在正确的位置后，确保在 `~/.zshrc` 中启用了补全功能：

```zsh
autoload -Uz compinit
compinit
```

##### Fish

Fish shell 会自动加载 `~/.config/fish/completions` 目录下的补全脚本，无需额外配置。

##### PowerShell

在 PowerShell 配置文件中添加：

```powershell
. $PROFILE\..\Completions\rdbkp2.ps1
```

## 命令行参数

### 通用参数

| 参数                | 描述                    | 默认值                         |
|---------------------|-------------------------|--------------------------------|
| `-y, --yes`         | 自动确认                | `false`                        |
| `-i, --interactive` | 使用交互式模式          | `true`                         |
| `-v, --verbose`     | 显示详细日志            | `false`                        |
| `-t, --timeout`     | 停止容器超时时间 (秒)   | `30`                           |
| `-e, --exclude`     | 排除模式                | `".git,node_modules,target"`   |
| `-r, --restart`     | 操作后重启容器          | `false`                        |

### 备份命令 (backup)

| 参数                | 描述                    |
|---------------------|-------------------------|
| `-c, --container`   | 容器名称或 ID           |
| `-f, --file`        | 需要备份的文件 (夹) 路径|
| `-o, --output`      | 输出目录                |
|                     | 继承自通用参数          |
| `-i, --interactive` | 使用交互式模式          |
| `-r, --restart`     | 操作后重启容器          |
| `-t, --timeout`     | 停止容器超时时间 (秒)   |
| `-e, --exclude`     | 排除模式                |

### 恢复命令 (restore)

> [!CAUTION]
> 💖 Restore the docker container binding Volume need Administrator privileges. <br>
> ✅ Please run [program] as sudo / RunAsAdminsitrator 

| 参数                | 描述                    |
|---------------------|-------------------------|
| `-c, --container`   | 容器名称或 ID           |
| `-f, --file`        | 备份文件 (压缩包) 路径  |
| `-o, --output`      | 输出目录                |
|                     | 继承自通用参数          |
| `-i, --interactive` | 使用交互式模式          |
| `-r, --restart`     | 操作后重启容器          |
| `-t, --timeout`     | 停止容器超时时间 (秒)   |
| ~~`-e, --exclude`~~ | ~~排除模式~~            |

### 列表命令 (list)

无参数，显示所有可用的容器。

### 补全命令 (completions)

- `shell`: 指定 shell 类型（bash/zsh/fish/powershell）

## 注意事项

1. 使用 Restore 功能时请确保使用 sudo / Administrator 权限进行操作
    - 更改，覆盖 Docker 容器挂载的 Volume(s) 时需要该权限进行写入操作 
1. 确保有足够的磁盘空间用于备份
2. 建议在恢复数据之前先备份当前数据
3. 需要有访问 Docker daemon 的权限
4. Windows 用户需要确保 Docker Desktop 已启动
