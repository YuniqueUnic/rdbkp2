# Docker Container Data Backup Tool

[English README.md](./README.md)

一个用于备份和恢复 Docker 容器数据的命令行工具。

## 功能特点

- 支持备份和恢复 Docker 容器的数据卷
- 支持命令行参数和交互式操作
- 使用 XZ 压缩算法进行高效压缩
- 支持命令行补全（Bash/Zsh/Fish/PowerShell）
- ~~支持自定义配置文件~~

## 安装

确保你的系统已安装 Rust 工具链，然后执行：

```bash
# 安装 rdbkp2
cargo install rdbkp2                                

# 创建软链接, 以实现 sudo rdbkp2 ... 的用法
# sudo ln -s $(where rdbkp2) /usr/local/bin/rdbkp2  # 创建 rdbkp2 的软链接到 /usr/local/bin/rdbkp2, 以实现 sudo rdbkp2 ... 的用法
rdbkp2 link install                                 # 使用该指令取代上面的手动创建软链接

# 检查更新
rdbkp2 update

# 卸载 rdbkp2
rdbkp2 uninstall
```

## 使用方法

### 列出可用的容器

```bash
rdbkp2 list
```

### 备份容器数据

> [!TIP]
> 按照以下优先级选择默认的备份目录：
> 1. $APPDATA/rdbkp2 (Windows) 或 ~/.local/share/rdbkp2 (Unix)
> 2. $HOME/rdbkp2
> 3. ./rdbkp2 (当前目录)

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

### 创建/删除软链接

```bash
rdbkp2 link install             # create the symbol-link at /usr/local/bin/rdbkp2
rdbkp2 link uninstall           # remove the symbol-link at /usr/local/bin/rdbkp2
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

| 参数                | 描述                              | 默认值                         |
|---------------------|-----------------------------------|--------------------------------|
| `-y, --yes`         | 自动确认                          | `false`                        |
| `-i, --interactive` | 使用交互式模式                    | `true`                         |
| `-v, --verbose`     | 显示详细日志                      | `false`                        |
| `-t, --timeout`     | 停止容器超时时间 (秒)             | `30`                           |
| `-e, --exclude`     | 排除模式                          | `".git,node_modules,target"`   |
| `-r, --restart`     | 操作后重启容器                    | `false`                        |
| `-l, --lang`        | 语言 (zh-CN/en/ja/ko/es/fr/de/it) | `zh-CN`                        |

### 备份命令 (backup)

| 参数                | 描述                              |
|---------------------|-----------------------------------|
| `-c, --container`   | 容器名称或 ID                     |
| `-f, --file`        | 需要备份的文件 (夹) 路径          |
| `-o, --output`      | 输出目录                          |
|                     | 继承自通用参数                    |
| `-y, --yes`         | 自动确认                          |
| `-i, --interactive` | 使用交互式模式                    |
| `-r, --restart`     | 操作后重启容器                    |
| `-t, --timeout`     | 停止容器超时时间 (秒)             |
| `-e, --exclude`     | 排除模式                          |
| `-l, --lang`        | 语言 (zh-CN/en/ja/ko/es/fr/de/it) |

### 恢复命令 (restore)

> [!CAUTION]
> 💖 Restore the docker container binding Volume need Administrator privileges. <br>
> ✅ Please run [program] as sudo / RunAsAdminsitrator 

| 参数                | 描述                              |
|---------------------|-----------------------------------|
| `-c, --container`   | 容器名称或 ID                     |
| `-f, --file`        | 备份文件 (压缩包) 路径            |
| `-o, --output`      | 输出目录                          |
|                     | 继承自通用参数                    |
| `-y, --yes`         | 自动确认                          |
| `-i, --interactive` | 使用交互式模式                    |
| `-r, --restart`     | 操作后重启容器                    |
| `-t, --timeout`     | 停止容器超时时间 (秒)             |
| ~~`-e, --exclude`~~ | ~~排除模式~~                      |
| `-l, --lang`        | 语言 (zh-CN/en/ja/ko/es/fr/de/it) | 

### 列表命令 (list)

无参数，显示所有可用的容器。

### 补全命令 (completions)

- `shell`: 指定 shell 类型（bash/zsh/fish/powershell）

### Link 及其子命令 (`Link install/uninstall`)

> [!CAUTION]
> 💖 **注意**: 安装软符号链接需要管理员权限。

| 参数                | 描述                              |
|---------------------|-----------------------------------|
|                     | 继承自通用参数                    |
| `-y, --yes`         | 自动确认                          |
| `-l, --lang`        | 语言 (zh-CN/en/ja/ko/es/fr/de/it) | 

## 注意事项

1. 使用 Restore 功能时请确保使用 sudo / Administrator 权限进行操作
    - 更改，覆盖 Docker 容器挂载的 Volume(s) 时需要该权限进行写入操作 
1. 确保有足够的磁盘空间用于备份
2. 建议在恢复数据之前先备份当前数据
3. 需要有访问 Docker daemon 的权限
4. Windows 用户需要确保 Docker Desktop 已启动

## 致谢

| 库名               | 版本      | 用途描述                                                                 | 链接                                      |
|--------------------|-----------|--------------------------------------------------------------------------|-------------------------------------------|
| **clap**           | 4.5.1     | CLI 参数解析与构建                                                       | [Crates.io](https://crates.io/crates/clap) |
| **dialoguer**      | 0.11.0    | CLI 交互式对话工具                                                       | [Crates.io](https://crates.io/crates/dialoguer) |
| **bollard**        | 0.18      | Docker API 客户端（支持SSL）                                             | [Crates.io](https://crates.io/crates/bollard) |
| **toml**           | 0.8.10    | TOML 格式配置文件解析                                                   | [Crates.io](https://crates.io/crates/toml) |
| **serde**          | 1.0       | 数据序列化/反序列化（带derive支持）                                      | [Crates.io](https://crates.io/crates/serde) |
| **tar**            | 0.4.40    | TAR 压缩/解压                                                           | [Crates.io](https://crates.io/crates/tar) |
| **xz2**            | 0.1.7     | XZ 压缩/解压                                                            | [Crates.io](https://crates.io/crates/xz2) |
| **anyhow**         | 1.0.80    | 错误处理与传播                                                         | [Crates.io](https://crates.io/crates/anyhow) |
| **thiserror**      | 2         | 自定义错误类型                                                         | [Crates.io](https://crates.io/crates/thiserror) |
| **tokio**          | 1.44      | 异步运行时（带full特性）                                                | [Crates.io](https://crates.io/crates/tokio) |
| **tracing**        | 0.1.40    | 日志追踪系统                                                           | [Crates.io](https://crates.io/crates/tracing) |
| **tracing-subscriber** | 0.3.18 | 日志订阅与格式化（带环境过滤）                                         | [Crates.io](https://crates.io/crates/tracing-subscriber) |
| **walkdir**        | 2.4.0     | 文件系统遍历                                                           | [Crates.io](https://crates.io/crates/walkdir) |
| **chrono**         | 0.4.34    | 日期与时间处理                                                         | [Crates.io](https://crates.io/crates/chrono) |
| **tempfile**       | 3.18      | 临时文件操作                                                           | [Crates.io](https://crates.io/crates/tempfile) |
| **fs_extra**       | 1.3.0     | 文件系统扩展操作                                                       | [Crates.io](https://crates.io/crates/fs_extra) |
| **dunce**          | 1.0.5     | 文件路径规范化                                                         | [Crates.io](https://crates.io/crates/dunce) |
| **mockall**        | 0.13.1    | 单元测试 Mock 工具                                                     | [Crates.io](https://crates.io/crates/mockall) |
| **privilege**      | 0.3.0     | 权限管理（用于Windows提权）                                             | [Crates.io](https://crates.io/crates/privilege) |
| **dirs**           | 6.0.0     | 系统目录路径获取                                                       | [Crates.io](https://crates.io/crates/dirs) |
| **semver**         | 1.0       | 语义化版本解析                                                         | [Crates.io](https://crates.io/crates/semver) |
| **reqwest**        | 0.12      | HTTP 请求客户端（带JSON支持）                                           | [Crates.io](https://crates.io/crates/reqwest) |
| **rust-i18n**      | 3.1.3     | 国际化与本地化支持                                                     | [Crates.io](https://crates.io/crates/rust-i18n) |
| **runas**          | 1.2.0     | Windows 提权运行命令（仅限Windows平台）                                 | [Crates.io](https://crates.io/crates/runas) |

### 说明：
1. **平台特定依赖**：
   - `runas` 仅用于 Windows 平台，其他平台无特殊依赖。
   - 其他库为通用依赖，支持跨平台（Linux/macOS/Windows）。

2. **性能优化**：
   - `strip = true`：在发布版本中移除调试符号，减小二进制体积。
   - `lto = "thin"` 和 `opt-level = 3`：启用链接时优化（LTO）和最高优化级别。

3. **致谢**：
   感谢以上开源项目为 `rdbkp2` 提供的基础设施支持！