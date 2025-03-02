pub(crate) mod out;

use anyhow::Result;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

use crate::log_bail;

/// 压缩目录/文件 (列表)，并在压缩包中添加额外的内存文件
///
/// # Arguments
///
/// * `sources` - 要压缩的源目录或文件路径 (列表)
/// * `output_file` - 压缩后的输出文件路径
/// * `memory_files` - 要添加到压缩包中的额外的内存文件列表，每个元素是一个元组 (文件名，文件内容)
/// * `exclude_patterns` - 要排除的文件/目录模式列表，为空则不排除
///
/// # Returns
///
/// * `Result<()>` - 成功返回 Ok(()), 失败返回 Err
///
/// # Examples
///
/// ```ignore
/// let source = Path::new("./source_dir");
/// let output = Path::new("output.tar.xz");
/// let memory_files = vec![("test.txt", "Hello World")];
/// let excludes = vec![".git", "node_modules"];
/// // let non-excludes = vec![];
/// compress_with_memory_file(source, output, &memory_files, &excludes)?;
/// ```
pub fn compress_with_memory_file<P: AsRef<Path>>(
    sources: &[P],
    output_file: P,
    memory_files: &[(&str, &str)],
    exclude_patterns: &[&str],
) -> Result<()> {
    let output_file = output_file.as_ref();

    let sources_item = sources
        .iter()
        .map(|s| s.as_ref().to_string_lossy())
        .collect::<Vec<_>>();
    info!(
        sources = ?sources_item,
        output_file = ?output_file,
        "Starting items compression"
    );

    let file = File::create(output_file).map_err(|e| {
        error!(?e, ?output_file, "Failed to create output file");
        e
    })?;

    let xz = XzEncoder::new(file, 9);
    let mut tar = tar::Builder::new(xz);
    debug!("Creating XZ encoder with compression level 9");

    let mut items_count = 0;

    // 首先添加内存中的文件
    items_count += append_memory_files(memory_files, &mut tar)?;

    // 处理每个源目录/文件
    for source in sources {
        // 然后添加源目录/文件
        items_count += append_items(source, exclude_patterns, &mut tar)?;
    }

    debug!("Finalizing archive");
    tar.finish().map_err(|e| {
        error!(?e, "Failed to finalize archive");
        e
    })?;

    info!(
        items_count,
        sources = ?sources_item,
        output_file = ?output_file,
        "Items compression completed successfully"
    );

    Ok(())
}

fn append_items<P: AsRef<Path>>(
    source: P,
    exclude_patterns: &[&str],
    tar: &mut tar::Builder<XzEncoder<File>>,
) -> Result<usize> {
    let mut items_count = 0;
    let source = source.as_ref();

    if source.is_dir() {
        let walker = WalkDir::new(source)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                let path = e.path().to_string_lossy();
                let excluded = exclude_patterns.iter().any(|p| path.contains(p));
                if excluded {
                    debug!(path = ?e.path(), "Excluding path");
                }
                !excluded
            });

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.path().is_file() {
                let name = entry
                    .path()
                    .strip_prefix(source.parent().unwrap_or(source))?;
                debug!(path = ?entry.path(), name = ?name, "Adding file to archive");
                tar.append_path_with_name(entry.path(), name)?;
                items_count += 1;
            }
        }
    } else if source.is_file() {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Failed to get file name"))?;
        debug!(path = ?source, name = ?name, "Adding file to archive");
        tar.append_path_with_name(source, name)?;
        items_count += 1;
    }

    Ok(items_count)
}

fn append_memory_files(
    memory_files: &[(&str, &str)],
    tar: &mut tar::Builder<XzEncoder<File>>,
) -> Result<usize> {
    for (name, content) in memory_files {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, content.as_bytes())?;
    }
    Ok(memory_files.len())
}

/// 解压缩 tar.xz 格式的归档文件到指定目录
///
/// # Arguments
///
/// * `archive_path` - 要解压的归档文件路径
/// * `target_dir` - 解压的目标目录路径
///
/// # Returns
///
/// 返回 `Result<()>`。如果解压成功则返回 `Ok(())`，否则返回相应的错误
///
/// # Errors
///
/// 此函数在以下情况会返回错误：
/// - 无法打开归档文件
/// - 无法创建 XZ 解码器
/// - 解压过程中出现错误
pub fn unpack_archive<P: AsRef<Path>>(archive_path: P, target_dir: P) -> Result<()> {
    let archive_path = archive_path.as_ref();
    let target_dir = target_dir.as_ref();

    info!(?archive_path, ?target_dir, "Starting archive extraction");

    let file = File::open(archive_path).map_err(|e| {
        error!(?e, ?archive_path, "Failed to open archive file");
        e
    })?;

    debug!("Creating XZ decoder");
    let xz = XzDecoder::new(file);
    let mut archive = tar::Archive::new(xz);

    debug!(?target_dir, "Unpacking archive");
    ensure_dir_exists(target_dir)?;

    // Unpack each entry while preserving paths
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let target_path = target_dir.join(path);

        if let Some(parent) = target_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        debug!(path = ?target_path, "Extracting file");
        entry.unpack(&target_path)?;
    }

    info!(
        ?archive_path,
        ?target_dir,
        "Archive extraction completed successfully"
    );
    Ok(())
}

/// 从压缩包中读取指定文件的内容
pub fn read_file_from_archive<P: AsRef<Path>>(archive_path: P, file_name: &str) -> Result<String> {
    let file = File::open(archive_path.as_ref())?;
    let xz = XzDecoder::new(file);
    let mut archive = tar::Archive::new(xz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == file_name {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return Ok(content);
        }
    }

    anyhow::bail!("File not found in archive: {}", file_name)
}

pub fn create_timestamp_filename(prefix: &str, ext: &str) -> String {
    use chrono::Local;
    let filename = format!("{}_{}{}", prefix, Local::now().format("%Y%m%d_%H%M%S"), ext);
    debug!(?filename, "Created timestamp filename");
    filename
}

/// 获取指定路径下以指定前缀开头的文件列表 (递归或非递归)
///
/// # Arguments
///
/// * `path`: 要搜索的路径。
/// * `prefix`: 文件名前缀。
/// * `recursive`: 是否递归搜索子目录。如果为 `true`，则递归搜索所有子目录；如果为 `false`，则只搜索当前目录。
///
/// # Returns
///
/// 返回一个 `Result`，包含一个 `PathBuf` 向量，其中包含所有匹配的文件路径。
/// 如果发生错误，则返回 `Err`，例如路径不存在或无法访问。
pub fn get_files_start_with<P: AsRef<Path>>(
    path: P,
    prefix: &str,
    recursive: bool,
) -> Result<Vec<PathBuf>> {
    let path_ref = path.as_ref();
    let mut files = Vec::new();

    if recursive {
        debug!(path = ?path_ref, prefix, "开始递归搜索文件");
        for entry in WalkDir::new(path_ref).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let file_name = entry.file_name().to_string_lossy();
                if file_name.starts_with(prefix) {
                    files.push(entry.path().to_path_buf());
                    debug!(file_path = ?entry.path(), "找到匹配文件 (递归)");
                }
            } else if entry.file_type().is_dir() && entry.depth() > 0 {
                debug!(dir_path = ?entry.path(), "进入子目录");
            }
        }
    } else {
        debug!(path = ?path_ref, prefix, "开始非递归搜索文件");
        match fs::read_dir(path_ref) {
            Ok(entries) => {
                for entry_result in entries {
                    match entry_result {
                        Ok(entry) => {
                            let file_type = entry.file_type()?;
                            if file_type.is_file() {
                                let entry_name = entry.file_name();
                                let file_name = entry_name.to_string_lossy();
                                if file_name.starts_with(prefix) {
                                    files.push(entry.path());
                                    debug!(file_path = ?entry.path(), "找到匹配文件 (非递归)");
                                }
                            } else if file_type.is_dir() {
                                debug!(dir_path = ?entry.path(), "忽略子目录 (非递归)");
                            }
                        }
                        Err(e) => {
                            warn!(error = ?e, path = ?path_ref, "读取目录条目失败");
                            // 在非递归模式下，单个条目读取失败不应终止整个函数，记录 warning 并继续
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, path = ?path_ref, "读取目录失败");
                return Err(anyhow::anyhow!(e)); // 直接返回 read_dir 的错误
            }
        }
    }

    if files.is_empty() {
        debug!(path = ?path_ref, prefix, recursive, "未找到任何匹配文件");
    } else {
        debug!(path = ?path_ref, prefix, recursive, file_count = files.len(), "找到匹配文件");
    }

    Ok(files)
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
pub fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> Result<()> {
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
pub fn ensure_file_exists<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::{TempDir, prelude::*};
    use predicates::prelude::*;

    fn assert_content_match<P: AsRef<Path>>(a: P, b: P) -> Result<()> {
        let a = a.as_ref();
        let b = b.as_ref();

        let a_file = fs::read_to_string(a).expect("Failed to read file");
        let b_file = fs::read_to_string(b).expect("Failed to read file");
        assert_eq!(a_file, b_file);
        Ok(())
    }

    #[test]
    fn test_create_timestamp_filename() {
        let filename = create_timestamp_filename("test", ".txt");
        assert!(filename.starts_with("test_"));
        assert!(filename.ends_with(".txt"));
        assert_eq!(filename.len(), 24); // test_YYYYMMDD_HHMMSS.txt
    }

    #[test]
    fn test_ensure_dir_exists() -> Result<()> {
        let temp = TempDir::new()?;
        let test_dir = temp.child("test_dir");

        ensure_dir_exists(&test_dir)?;
        test_dir.assert(predicate::path::exists());

        // 测试重复创建
        ensure_dir_exists(&test_dir)?;
        test_dir.assert(predicate::path::exists());

        Ok(())
    }

    #[test]
    fn test_compress_and_extract() -> Result<()> {
        let temp = TempDir::new()?;

        // 创建测试文件
        let source_dir = temp.child("source");
        source_dir.create_dir_all()?;

        let test_file = source_dir.child("test.txt");
        test_file.write_str("Hello, World!")?;

        // 压缩
        let archive = temp.child("archive.tar.xz");
        compress_with_memory_file(&[&source_dir], &archive, &[], &[])?;
        archive.assert(predicate::path::exists());

        // 解压
        let extract_dir = temp.child("extract");
        extract_dir.create_dir_all()?;
        unpack_archive(&archive, &extract_dir)?;

        // 验证
        let extracted_file = extract_dir.child(format!("{}/{}", "source", "test.txt"));
        extracted_file.assert(predicate::path::exists());
        extracted_file.assert(predicate::str::contains("Hello, World!"));

        Ok(())
    }

    #[test]
    fn test_compress_and_extract_with_input() -> Result<()> {
        let temp = TempDir::new()?;
        let source = temp.child("source");
        let extract = temp.child("extract");
        source.create_dir_all()?;
        extract.create_dir_all()?;

        let content = "Hello, World! ";
        let file = source.child("test.txt");
        file.write_str(content)?;

        let archive_path = temp.child("archive.tar.xz");
        compress_with_memory_file(&[&source], &archive_path, &[], &[])?;
        unpack_archive(&archive_path, &extract)?;
        assert_content_match(
            &file,
            &extract.child(format!(
                "{}/{}",
                "source",
                file.file_name().unwrap().to_string_lossy()
            )),
        )?;

        Ok(())
    }

    #[test]
    fn test_read_file_from_archive() -> Result<()> {
        let temp = TempDir::new()?;
        let archive = temp.child("test.tar.xz");

        // 创建一个包含内存文件的压缩包
        let test_content = "Hello from memory file!";
        let memory_files = vec![("test.txt", test_content)];
        compress_with_memory_file(&[temp.path()], &archive, &memory_files, &[])?;

        // 从压缩包中读取文件
        let content = read_file_from_archive(&archive, "test.txt")?;
        assert_eq!(content, test_content);

        // 测试读取不存在的文件
        let result = read_file_from_archive(&archive, "nonexistent.txt");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_compress_with_memory_file() -> Result<()> {
        let temp = TempDir::new()?;

        // 创建源文件
        let source_dir = temp.child("source");
        source_dir.create_dir_all()?;
        let test_file = source_dir.child("source.txt");
        test_file.write_str("Source file content")?;

        // 创建压缩包
        let archive = temp.child("archive.tar.xz");
        let memory_files = vec![
            ("memory1.txt", "Memory file 1 content"),
            ("memory2.txt", "Memory file 2 content"),
        ];
        compress_with_memory_file(&[&source_dir], &archive, &memory_files, &[])?;

        // 验证压缩包内容
        let extract_dir = temp.child("extract");
        extract_dir.create_dir_all()?;
        unpack_archive(&archive, &extract_dir)?;

        // 检查内存文件
        let memory_file1 = extract_dir.child("memory1.txt");
        memory_file1.assert(predicate::path::exists());
        memory_file1.assert(predicate::str::contains("Memory file 1 content"));

        let memory_file2 = extract_dir.child("memory2.txt");
        memory_file2.assert(predicate::path::exists());
        memory_file2.assert(predicate::str::contains("Memory file 2 content"));

        // 检查源文件
        let source_file = extract_dir.child(format!("{}/{}", "source", "source.txt"));
        source_file.assert(predicate::path::exists());
        source_file.assert(predicate::str::contains("Source file content"));

        Ok(())
    }
}
