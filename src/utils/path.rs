use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Result;
use tracing::{debug, error, info};

use crate::log_bail;

/// 获取默认的备份目录
///
/// 按照以下优先级选择备份目录：
/// 1. APPDATA/rdbkp2 (Windows) 或 ~/.local/share/rdbkp2 (Unix)
/// 2. HOME/rdbkp2
/// 3. ./rdbkp2 (当前目录)
pub(crate) fn get_default_backup_dir() -> PathBuf {
    let backup_dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| {
            tracing::warn!("{}", t!("utils.path.failed_to_get_system_dir"));
            PathBuf::from(".")
        })
        .join("rdbkp2");

    if let Err(err) = fs::create_dir_all(&backup_dir) {
        tracing::warn!(
            "{}: {}, {}",
            t!("utils.path.failed_to_create_backup_dir"),
            err,
            t!("utils.path.use_current_dir")
        );
        return PathBuf::from("./rdbkp2");
    }

    tracing::debug!("{}: {}", t!("utils.path.backup_dir"), backup_dir.display());
    backup_dir
}

/// 确保目录存在，如果不存在则创建
///
/// # Arguments
///
/// * `path` - 要确保存在的目录路径。如果路径包含文件扩展名，则创建其父目录
///
/// # Returns
///
/// * `Result<()>` - 成功返回 Ok(()), 失败返回 Err
///
/// # Examples
///
/// ```ignore
/// use std::path::Path;
/// use crate::utils::ensure_dir_exists;
/// ensure_dir_exists(Path::new("/tmp/test"))?; // 创建目录
/// ensure_dir_exists(Path::new("/tmp/test/file.txt"))?; // 创建父目录
/// ```
pub(crate) fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    debug!(path = ?path, "Ensuring directory exists");

    if !path.exists() {
        debug!(?path, "Creating directory");

        if path.extension().is_none() {
            // 如果路径没有扩展名，视为目录路径，创建所有必需目录
            std::fs::create_dir_all(path).map_err(|e| {
                error!(?e, ?path, "Failed to create directory");
                e
            })?;
        } else {
            // 如果路径有扩展名，视为文件路径，创建所有必需的父目录
            let parent_dir = path.parent().ok_or_else(|| {
                anyhow::anyhow!("Failed to get parent directory: {}", path.display())
            })?;

            std::fs::create_dir_all(parent_dir).map_err(|e| {
                error!(?e, ?path, "Failed to create directory");
                e
            })?;
        }

        info!(?path, "Directory created successfully");
    } else {
        debug!(?path, "Directory already exists");
    }
    Ok(())
}

/// 确保文件存在
///
/// # Arguments
///
/// * `path` - 要确保存在的文件路径。
///
/// # Returns
///
/// * `Result<PathBuf>` - 成功返回 Ok(PathBuf)，失败返回 Err
pub(crate) fn ensure_file_exists<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    debug!(path = ?path, "Ensuring file exists");

    let file = PathBuf::from(path);
    if !file.exists() || !file.is_file() {
        log_bail!(
            "ERROR",
            "File does not exist or is not a file: {}",
            file.to_string_lossy()
        );
    }
    Ok(file)
}

/// 将路径转换为绝对路径并尽可能规范化 (简单版)
/// - 如果是相对路径，则基于当前工作目录转换为绝对路径
/// - 尝试执行 canonicalize（解析符号链接并处理冗余）
/// - 如果路径不存在，报错路径不存在
pub(crate) fn absolute_canonicalize_path(path: &Path) -> io::Result<PathBuf> {
    // 1. 检查路径是否已经是绝对路径
    if path.is_absolute() {
        // 如果已经是绝对路径，则直接 canonicalize
        path.canonicalize()
    } else {
        // 如果不是绝对路径，先获取当前工作目录
        let current_dir = std::env::current_dir()?;
        // 将相对路径转换为相对于当前工作目录的绝对路径
        let absolute_path = current_dir.join(path);
        // 然后 canonicalize 绝对路径
        absolute_path.canonicalize()
    }
}

#[allow(dead_code)]
/// 将路径转换为绝对路径并尽可能规范化
/// - 如果是相对路径，则基于当前工作目录转换为绝对路径
/// - 尝试执行 canonicalize（解析符号链接并处理冗余）
/// - 如果路径不存在，仍返回简化的绝对路径（处理冗余但保留不存在的部分）
pub(crate) fn ensure_absolute_canonical<P: AsRef<Path>>(
    path: P,
    base_path: P,
) -> io::Result<PathBuf> {
    let path = path.as_ref();
    let base_path = base_path.as_ref();

    // 转换为绝对路径
    let absolute = if path.is_absolute() {
        path
    } else {
        &base_path.join(path)
    };

    // 尝试规范化（解析符号链接）
    match dunce::canonicalize(absolute) {
        Ok(canonical) => Ok(canonical),
        Err(_) => {
            // 路径不存在时，手动处理冗余部分
            Ok(simplify_absolute_path(absolute))
        }
    }
}

#[allow(dead_code)]
/// 简化绝对路径的冗余部分（不依赖文件系统存在性）
fn simplify_absolute_path(path: &Path) -> PathBuf {
    let mut stack = Vec::new();
    let separator = OsString::from(std::path::MAIN_SEPARATOR.to_string());

    for component in path.components() {
        match component {
            std::path::Component::Prefix(p) => stack.push(p.as_os_str().to_owned()),
            std::path::Component::RootDir => {
                stack.push(std::path::MAIN_SEPARATOR.to_string().into())
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if stack.last().is_some_and(|s| s != &separator) {
                    stack.pop();
                }
            }
            std::path::Component::Normal(p) => stack.push(p.to_owned()),
        }
    }

    // 处理 Windows 的特殊前缀情况
    if cfg!(windows) && path.has_root() && stack.len() == 1 {
        stack.push(std::path::MAIN_SEPARATOR.to_string().into());
    }

    stack.iter().collect()
}

#[cfg(test)]
mod tests {
    use assert_fs::prelude::*;

    use super::*;
    use std::fs::File;

    #[test]
    fn test_absolute_conversion() {
        let rel_path = Path::new("test.txt");
        #[cfg(not(target_os = "windows"))]
        let base_path = Path::new("/base/path");
        #[cfg(target_os = "windows")]
        let base_path = Path::new(r#"C:\base\path"#);
        let abs_path = ensure_absolute_canonical(rel_path, base_path).unwrap();
        assert!(abs_path.is_absolute());
    }

    #[test]
    fn test_redundant_components() {
        let path = Path::new("/foo/./bar//../baz");
        let simplified = simplify_absolute_path(path);
        assert_eq!(simplified, PathBuf::from("/foo/baz"));
    }

    #[test]
    fn test_absolute_canonicalize_path_with_tempdir() -> anyhow::Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        // 1. 测试相对路径
        let existing_filename = "README.md";
        let rel_path = Path::new(existing_filename);
        let abs_path = absolute_canonicalize_path(rel_path)?;
        assert!(abs_path.is_absolute());
        assert_eq!(
            abs_path,
            std::env::current_dir()?
                .canonicalize()?
                .join(existing_filename)
                .canonicalize()?
        ); // 检查是否与预期 canonicalized 路径一致
        println!("相对路径：{:?}", rel_path);
        println!("Canonicalized 绝对路径 (相对): {:?}", abs_path);

        // 2. 测试绝对路径 (在 TempDir 中创建文件)
        let absolute_file = temp_dir.child("absolute_test_file.txt");
        File::create(&absolute_file)?;
        let abs_path_input = absolute_file.path();
        let canonical_absolute_path = absolute_canonicalize_path(abs_path_input)?;
        assert!(canonical_absolute_path.is_absolute());
        assert_eq!(canonical_absolute_path, abs_path_input.canonicalize()?); // 绝对路径 canonicalize 后应该还是自身
        println!("绝对路径：{:?}", abs_path_input);
        println!(
            "Canonicalized 绝对路径 (绝对): {:?}",
            canonical_absolute_path
        );

        // 3. 测试符号链接路径
        let real_file = temp_dir.child("real_file.txt");
        File::create(&real_file)?;
        let link_path = temp_dir.child("symlink_to_real_file");
        link_path.symlink_to_file(real_file.path())?; // 创建文件符号链接

        let canonical_symlink_path = absolute_canonicalize_path(link_path.path())?;
        assert!(canonical_symlink_path.is_absolute());
        assert_eq!(canonical_symlink_path, real_file.path().canonicalize()?); // 符号链接 canonicalize 后应该指向真实文件
        println!("符号链接路径：{:?}", link_path.path());
        println!(
            "Canonicalized 绝对路径 (符号链接): {:?}",
            canonical_symlink_path
        );

        // 4. 处理不存在的文件或目录 (canonicalize 会报错)
        let non_existent_path = temp_dir.child("non_existent_dir/file.txt");
        let result = absolute_canonicalize_path(non_existent_path.path());
        assert!(result.is_err()); // 期待 canonicalize 失败
        match result {
            Ok(canonical_path) => println!("Canonicalized 路径 (不存在): {:?}", canonical_path), // 不应该执行到这里
            Err(e) => {
                eprintln!(
                    "Error canonicalizing path {:?}: {}",
                    non_existent_path.path(),
                    e
                );
                assert_eq!(e.kind(), io::ErrorKind::NotFound); // 检查错误类型是否为 NotFound (或其他相关错误，取决于系统)
            }
        }

        temp_dir.close()?; // 手动关闭 TempDir，虽然 Drop 会自动处理，但显式关闭更清晰
        Ok(())
    }
}
