use anyhow::Result;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

pub fn compress<P: AsRef<Path>>(
    source: P,
    output_file: P,
    exclude_patterns: &[&str],
) -> Result<()> {
    let source = source.as_ref();
    let output_file = output_file.as_ref();

    info!(
        source = ?source,
        output_file = ?output_file,
        "Starting directory compression"
    );

    let file = File::create(output_file).map_err(|e| {
        error!(?e, ?output_file, "Failed to create output file");
        e
    })?;

    debug!("Creating XZ encoder with compression level 9");
    let xz = XzEncoder::new(file, 9);
    let mut tar = tar::Builder::new(xz);

    debug!(
        ?exclude_patterns,
        "Setting up file walker with exclusion patterns"
    );
    let walker = walkdir::WalkDir::new(source)
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

    let mut file_count = 0;
    if source.is_dir() {
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                debug!(path = ?path, "Adding file to archive");
                let name = path.strip_prefix(source).map_err(|e| {
                    error!(?e, ?path, "Failed to strip prefix from path");
                    e
                })?;
                tar.append_path_with_name(path, name).map_err(|e| {
                    error!(?e, ?path, "Failed to add file to archive");
                    e
                })?;
                file_count += 1;
            }
        }
    } else if source.is_file() {
        debug!(path = ?source, "Adding file to archive");
        let name = source.file_name().ok_or_else(|| {
            error!("Failed to get file name");
            anyhow::anyhow!("Failed to get file name")
        })?;
        tar.append_path_with_name(source, name).map_err(|e| {
            error!(?e, ?source, "Failed to add file to archive");
            e
        })?;
        file_count += 1;
    }

    debug!("Finalizing archive");
    tar.finish().map_err(|e| {
        error!(?e, "Failed to finalize archive");
        e
    })?;

    info!(
        file_count,
        source_dir = ?source,
        output_file = ?output_file,
        "Directory compression completed successfully"
    );
    Ok(())
}

#[deprecated]
#[allow(dead_code)]
fn compress_dir<P: AsRef<Path>>(
    source_dir: P,
    output_file: P,
    exclude_patterns: &[&str],
) -> Result<()> {
    let source_dir = source_dir.as_ref();
    let output_file = output_file.as_ref();

    info!(
        source_dir = ?source_dir,
        output_file = ?output_file,
        "Starting directory compression"
    );

    let file = File::create(output_file).map_err(|e| {
        error!(?e, ?output_file, "Failed to create output file");
        e
    })?;

    debug!("Creating XZ encoder with compression level 9");
    let xz = XzEncoder::new(file, 9);
    let mut tar = tar::Builder::new(xz);

    debug!(
        ?exclude_patterns,
        "Setting up file walker with exclusion patterns"
    );
    let walker = walkdir::WalkDir::new(source_dir)
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

    let mut file_count = 0;
    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            debug!(path = ?path, "Adding file to archive");
            let name = path.strip_prefix(source_dir).map_err(|e| {
                error!(?e, ?path, "Failed to strip prefix from path");
                e
            })?;
            tar.append_path_with_name(path, name).map_err(|e| {
                error!(?e, ?path, "Failed to add file to archive");
                e
            })?;
            file_count += 1;
        }
    }

    debug!("Finalizing archive");
    tar.finish().map_err(|e| {
        error!(?e, "Failed to finalize archive");
        e
    })?;

    info!(
        file_count,
        source_dir = ?source_dir,
        output_file = ?output_file,
        "Directory compression completed successfully"
    );
    Ok(())
}

pub fn extract_archive<P: AsRef<Path>>(archive_path: P, target_dir: P) -> Result<()> {
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
    archive.unpack(target_dir).map_err(|e| {
        error!(?e, ?target_dir, "Failed to unpack archive");
        e
    })?;

    info!(
        ?archive_path,
        ?target_dir,
        "Archive extraction completed successfully"
    );
    Ok(())
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

pub fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    if !path.exists() {
        debug!(?path, "Creating directory");

        if path.extension().is_none() {
            std::fs::create_dir_all(path).map_err(|e| {
                error!(?e, ?path, "Failed to create directory");
                e
            })?;
        } else {
            let parent_dir = path.parent().ok_or_else(|| {
                anyhow::anyhow!("Failed to get parent directory: {}", path.display())
            })?;

            std::fs::create_dir(parent_dir).map_err(|e| {
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

    Err(anyhow::anyhow!("File not found in archive: {}", file_name))
}

/// 压缩目录/文件，并在压缩包中添加额外的内存文件
pub fn compress_with_memory_file<P: AsRef<Path>>(
    source: P,
    output_file: P,
    memory_files: &[(&str, &str)], // (文件名，文件内容)
    exclude_patterns: &[&str],
) -> Result<()> {
    let file = File::create(output_file.as_ref())?;
    let xz = XzEncoder::new(file, 9);
    let mut tar = tar::Builder::new(xz);

    // 首先添加内存中的文件
    for (name, content) in memory_files {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, content.as_bytes())?;
    }

    // 然后添加源目录/文件
    if source.as_ref().is_dir() {
        let walker = WalkDir::new(source.as_ref())
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                let path = e.path().to_string_lossy();
                !exclude_patterns.iter().any(|p| path.contains(p))
            });

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.path().is_file() {
                let name = entry.path().strip_prefix(source.as_ref())?;
                tar.append_path_with_name(entry.path(), name)?;
            }
        }
    } else if source.as_ref().is_file() {
        let name = source
            .as_ref()
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Failed to get file name"))?;
        tar.append_path_with_name(source.as_ref(), name)?;
    }

    tar.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
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
        let temp = assert_fs::TempDir::new()?;
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
        let temp = assert_fs::TempDir::new()?;

        // 创建测试文件
        let source_dir = temp.child("source");
        source_dir.create_dir_all()?;

        let test_file = source_dir.child("test.txt");
        test_file.write_str("Hello, World!")?;

        // 压缩
        let archive = temp.child("archive.tar.xz");
        compress(&source_dir, &archive, &[])?;
        archive.assert(predicate::path::exists());

        // 解压
        let extract_dir = temp.child("extract");
        extract_dir.create_dir_all()?;
        extract_archive(&archive, &extract_dir)?;

        // 验证
        let extracted_file = extract_dir.child("test.txt");
        extracted_file.assert(predicate::path::exists());
        extracted_file.assert(predicate::str::contains("Hello, World!"));

        Ok(())
    }

    #[test]
    fn test_compress_and_extract_with_input() -> Result<()> {
        let file = "./docker/Dockerfile";
        let archive_path = "./backups/dockerfile.tar.xz";
        let target_dir = "./backups/";
        ensure_dir_exists(target_dir)?;
        compress(file, archive_path, &[])?;
        extract_archive(archive_path, target_dir)?;

        let output = format!("{}/{}", target_dir, file.split('/').last().unwrap());
        assert_content_match(file, &output)?;

        fs::remove_file(archive_path)?;
        fs::remove_file(output)?;

        Ok(())
    }
}
