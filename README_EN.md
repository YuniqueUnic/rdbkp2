# Docker Container Data Backup Tool

[中文 README.md](./README.md)

A command-line tool for backing up and restoring Docker container data.

## Key Features

- Backs up and restores Docker container data volumes.
- Supports both command-line arguments and interactive operations.
- Employs XZ compression algorithm for efficient compression.
- Supports custom configuration files.
- Offers command-line completion for Bash, Zsh, Fish, and PowerShell.

## Installation

Ensure you have the Rust toolchain installed on your system, then execute:

```bash
cargo install rdbkp2                                # install rdbkp2
sudo ln -s $(which rdbkp2) /usr/local/bin/rdbkp2   # create symbolic link for sudo execution
```

## Usage

### Listing Available Containers

```bash
rdbkp2 list
```

### Backing Up Container Data

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

| Argument             | Description                      | Default Value                      |
|----------------------|---------------------------------|------------------------------------|
| `-y, --yes`          | Automatically confirm prompts     | `false`                            |
| `-i, --interactive`  | Use interactive mode            | `true`                             |
| `-v, --verbose`      | Display verbose logs             | `false`                            |
| `-t, --timeout`      | Container stop timeout (seconds) | `30`                               |
| `-e, --exclude`      | Exclude patterns                 | `".git,node_modules,target"`       |
| `-r, --restart`      | Restart container after operation | `false`                            |

### Backup Command (`backup`)

| Argument             | Description                          |
|----------------------|--------------------------------------|
| `-c, --container`    | Container name or ID                 |
| `-f, --file`         | Path to file(s) or folder(s) to backup |
| `-o, --output`       | Output directory                       |
|                      | Inherited from common arguments      |
| `-i, --interactive`  | Use interactive mode                 |
| `-r, --restart`      | Restart container after operation    |
| `-t, --timeout`      | Container stop timeout (seconds)     |
| `-e, --exclude`      | Exclude patterns                     |

### Restore Command (`restore`)

> [!CAUTION]
> 💖 **Caution**: Restoring Docker container bound volumes requires Administrator privileges. <br>
> ✅ Please run [program] as `sudo` / `Run as Administrator`.

| Argument             | Description                        |
|----------------------|------------------------------------|
| `-c, --container`    | Container name or ID               |
| `-f, --file`         | Path to backup file (compressed archive) |
| `-o, --output`       | Output directory                     |
|                      | Inherited from common arguments    |
| `-i, --interactive`  | Use interactive mode               |
| `-r, --restart`      | Restart container after operation  |
| `-t, --timeout`      | Container stop timeout (seconds)   |
| ~~`-e, --exclude`~~ | ~~Exclude patterns~~               |

### List Command (`list`)

No arguments. Displays all available containers.

### Completions Command (`completions`)

- `shell`: Specifies the shell type (bash/zsh/fish/powershell)

## Important Notes

1.  When using the Restore function, ensure you operate with `sudo` / Administrator privileges.
    -   This permission is required for write operations when changing and overwriting Docker container-mounted volumes.
2.  Ensure sufficient disk space is available for backups.
3.  It is recommended to back up your current data before restoring.
4.  You need to have permissions to access the Docker daemon.
5.  Windows users need to ensure Docker Desktop is running.
