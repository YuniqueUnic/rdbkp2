use anyhow::Result;
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::Builder;
use tracing::{debug, error, info, warn};
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

pub fn compress_directory<P: AsRef<Path>>(
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
    let mut tar = Builder::new(xz);

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

pub fn ensure_dir_exists<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        debug!(?path, "Creating directory");
        std::fs::create_dir_all(path).map_err(|e| {
            error!(?e, ?path, "Failed to create directory");
            e
        })?;
        info!(?path, "Directory created successfully");
    } else {
        debug!(?path, "Directory already exists");
    }
    Ok(())
}
