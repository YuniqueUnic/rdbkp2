# Docker Container Data Backup Tool

[中文 README.md](./README_CN.md)

A command-line tool for backing up and restoring Docker container data.

## Key Features

- Backs up and restores Docker container data volumes.
- Supports both command-line arguments and interactive operations.
- Employs XZ compression algorithm for efficient compression.
- Offers command-line completion for Bash, Zsh, Fish, and PowerShell.
- ~~Supports custom configuration files.~~

## Installation

Ensure you have the Rust toolchain installed on your system, then execute:

```bash
# Install rdbkp2
cargo install rdbkp2

# Create symbolic link to enable sudo rdbkp2 ... usage
# sudo ln -s $(where rdbkp2) /usr/local/bin/rdbkp2  # Create a symbolic link for rdbkp2 to /usr/local/bin/rdbkp2 to enable sudo rdbkp2 ... usage
rdbkp2 link install                                 # Use this command to replace manual symbolic link creation above

# Check for updates
rdbkp2 update

# Uninstall rdbkp2
rdbkp2 uninstall
```

## Usage

### Listing Available Containers

```bash
rdbkp2 list
```

### Backing Up Container Data

> [!TIP]
> The default backup directory is selected based on the following priority:
> 1. $APPDATA/rdbkp2 (Windows) or ~/.local/share/rdbkp2 (Unix)
> 2. $HOME/rdbkp2
> 3. ./rdbkp2 (current directory)

Interactive Mode:

```bash
rdbkp2 backup -i
```

Command-line Mode:

```bash
rdbkp2 backup -c container_name -o /path/to/backup/dir
```

### Restoring Container Data

Interactive Mode:

```bash
rdbkp2 restore -i
```

Command-line Mode:

```bash
rdbkp2 restore -c container_name -f /path/to/backup/file
```

### Create/Remove Symbol-link for program

```bash
rdbkp2 link install             # create the symbol-link at /usr/local/bin/rdbkp2
rdbkp2 link uninstall           # remove the symbol-link at /usr/local/bin/rdbkp2
```

### Command-Line Completion

Generate command-line completion scripts for various shells:

```bash
# Generate Bash completion script
rdbkp2 completions bash > ~/.local/share/bash-completion/completions/rdbkp2

# Generate Zsh completion script
rdbkp2 completions zsh > ~/.zsh/_rdbkp2

# Generate Fish completion script
rdbkp2 completions fish > ~/.config/fish/completions/rdbkp2.fish

# Generate PowerShell completion script
# Windows PowerShell
mkdir -p $PROFILE\..\Completions
rdbkp2 completions powershell > $PROFILE\..\Completions\rdbkp2.ps1
```

#### Enabling Completion Functionality

##### Bash

Add the following lines to your `~/.bashrc` or `~/.bash_profile`:

```bash
source ~/.local/share/bash-completion/completions/rdbkp2
```

##### Zsh

After placing the completion script in the correct location, ensure completion is enabled in your `~/.zshrc`:

```zsh
autoload -Uz compinit
compinit
```

##### Fish

Fish shell automatically loads completion scripts from the `~/.config/fish/completions` directory. No additional configuration is needed.

##### PowerShell

Add the following line to your PowerShell profile:

```powershell
. $PROFILE\..\Completions\rdbkp2.ps1
```

## Command-Line Arguments

### Common Arguments

| Argument             | Description                            | Default Value                      |
|----------------------|----------------------------------------|------------------------------------|
| `-y, --yes`          | Automatic confirmation prompt          | `false`                            |
| `-i, --interactive`  | Use interactive mode                   | `true`                             |
| `-v, --verbose`      | Display detailed logs                  | `false`                            |
| `-t, --timeout`      | Container stop timeout (seconds)       | `30`                               |
| `-e, --exclude`      | Exclusion patterns                     | `".git,node_modules,target"`       |
| `-r, --restart`      | Restart container after operation      | `false`                            |
| `-l, --lang`         | Language (zh-CN/en/ja/ko/es/fr/de/it)  | `zh-CN`                            |

### Backup Command (`backup`)

| Argument             | Description                                      |
|----------------------|--------------------------------------------------|
| `-c, --container`    | Container name or ID                             |
| `-f, --file`         | Path to file(s) or directory(s) to back up       |
| `-o, --output`       | Output directory                                 |
|                      | Inherited from common arguments                  |
| `-y, --yes`          | Automatic confirmation prompt                    |
| `-i, --interactive`  | Use interactive mode                             |
| `-r, --restart`      | Restart the container after operation            |
| `-t, --timeout`      | Timeout for stopping the container (seconds)     |
| `-e, --exclude`      | Exclusion patterns                               |
| `-l, --lang`         | Language (zh-CN/en/ja/ko/es/fr/de/it)            |

### Restore Command (`restore`)

> [!CAUTION]
> 💖 **Caution**: Restoring Docker container bound volumes requires Administrator privileges. <br>
> ✅ Please run [program] as `sudo` / `Run as Administrator`.

| Argument             | Description                                      |
|----------------------|--------------------------------------------------|
| `-c, --container`    | Container name or ID                             |
| `-f, --file`         | Path to backup file (compressed archive)         |
| `-o, --output`       | Output directory                                 |
|                      | Inherited from common arguments                  |
| `-y, --yes`          | Automatic confirmation prompt                    |
| `-i, --interactive`  | Use interactive mode                             |
| `-r, --restart`      | Restart container after operation                |
| `-t, --timeout`      | Container stop timeout (seconds)                 |
| ~~`-e, --exclude`~~  | ~~Exclude patterns~~                             |
| `-l, --lang`         | Language (zh-CN/en/ja/ko/es/fr/de/it)            |

### List Command (`list`)

No arguments. Displays all available containers.

### Completions Command (`completions`)

- `shell`: Specifies the shell type (bash/zsh/fish/powershell)

### Link SubCommand (`Link install/uninstall`)

> [!CAUTION]
> 💖 **Caution**: Install soft-symbol-link requires Administrator privileges.

| Argument             | Description                                      |
|----------------------|--------------------------------------------------|
|                      | Inherited from common arguments                  |
| `-y, --yes`          | Automatic confirmation prompt                    |
| `-l, --lang`         | Language (zh-CN/en/ja/ko/es/fr/de/it)            |

## Important Notes

1.  When using the Restore function, ensure you operate with `sudo` / Administrator privileges.
    -   This permission is required for write operations when changing and overwriting Docker container-mounted volumes.
2.  Ensure sufficient disk space is available for backups.
3.  It is recommended to back up your current data before restoring.
4.  You need to have permissions to access the Docker daemon.
5.  Windows users need to ensure Docker Desktop is running.

## Acknowledgments

| Library Name       | Version   | Purpose Description                                                         | Link                                      |
|--------------------|-----------|-----------------------------------------------------------------------------|-------------------------------------------|
| **clap**           | 4.5.1     | CLI argument parsing and construction                                       | [Crates.io](https://crates.io/crates/clap) |
| **dialoguer**      | 0.11.0    | CLI interactive dialogue tool                                               | [Crates.io](https://crates.io/crates/dialoguer) |
| **bollard**        | 0.18      | Docker API client (supports SSL)                                            | [Crates.io](https://crates.io/crates/bollard) |
| **toml**           | 0.8.10    | TOML format configuration file parsing                                      | [Crates.io](https://crates.io/crates/toml) |
| **serde**          | 1.0       | Data serialization/deserialization (with derive support)                    | [Crates.io](https://crates.io/crates/serde) |
| **tar**            | 0.4.40    | TAR compression/decompression                                               | [Crates.io](https://crates.io/crates/tar) |
| **xz2**            | 0.1.7     | XZ compression/decompression                                                | [Crates.io](https://crates.io/crates/xz2) |
| **anyhow**         | 1.0.80    | Error handling and propagation                                              | [Crates.io](https://crates.io/crates/anyhow) |
| **thiserror**      | 2         | Custom error type definition                                                | [Crates.io](https://crates.io/crates/thiserror) |
| **tokio**          | 1.44      | Asynchronous runtime (with full features)                                   | [Crates.io](https://crates.io/crates/tokio) |
| **tracing**        | 0.1.40    | Log tracing system                                                          | [Crates.io](https://crates.io/crates/tracing) |
| **tracing-subscriber** | 0.3.18 | Log subscription and formatting (with environment filtering)                | [Crates.io](https://crates.io/crates/tracing-subscriber) |
| **walkdir**        | 2.4.0     | File system traversal                                                       | [Crates.io](https://crates.io/crates/walkdir) |
| **chrono**         | 0.4.34    | Date and time handling                                                      | [Crates.io](https://crates.io/crates/chrono) |
| **tempfile**       | 3.18      | Temporary file operations                                                   | [Crates.io](https://crates.io/crates/tempfile) |
| **fs_extra**       | 1.3.0     | File system extended operations                                             | [Crates.io](https://crates.io/crates/fs_extra) |
| **dunce**          | 1.0.5     | File path normalization                                                     | [Crates.io](https://crates.io/crates/dunce) |
| **mockall**        | 0.13.1    | Unit test mocking tool                                                      | [Crates.io](https://crates.io/crates/mockall) |
| **privilege**      | 0.3.0     | Privilege management (for Windows privilege elevation)                      | [Crates.io](https://crates.io/crates/privilege) |
| **dirs**           | 6.0.0     | System directory path retrieval                                             | [Crates.io](https://crates.io/crates/dirs) |
| **semver**         | 1.0       | Semantic version parsing                                                    | [Crates.io](https://crates.io/crates/semver) |
| **reqwest**        | 0.12      | HTTP request client (with JSON support)                                     | [Crates.io](https://crates.io/crates/reqwest) |
| **rust-i18n**      | 3.1.3     | Internationalization and localization support                               | [Crates.io](https://crates.io/crates/rust-i18n) |
| **runas**          | 1.2.0     | Windows command execution with elevated privileges (Windows-only)           | [Crates.io](https://crates.io/crates/runas) |

### Notes:
1. **Platform-Specific Dependencies**:
   - `runas` is Windows-only; other libraries are cross-platform (Linux/macOS/Windows).

2. **Performance Optimization**:
   - `strip = true`: Removes debug symbols in release builds to reduce binary size.
   - `lto = "thin"` and `opt-level = 3`: Enables Link-Time Optimization (LTO) and maximum optimization level.

3. **Acknowledgements**:
   Special thanks to these open-source projects for providing foundational support to `rdbkp2`!