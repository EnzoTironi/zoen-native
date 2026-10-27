//! Bounded local reads and all-or-nothing, no-clobber publication.
use super::Result;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(super) fn read(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|_| format!("{label} unavailable"))?;
    if !file
        .metadata()
        .map_err(|_| format!("{label} metadata"))?
        .is_file()
    {
        return Err(format!("{label} must be a regular file"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{label} unreadable"))?;
    if bytes.is_empty() || u64::try_from(bytes.len()).map_err(|_| "input length")? > limit {
        return Err(format!("{label} empty or over size limit"));
    }
    Ok(bytes)
}

pub(super) fn publish_new(output: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if !output.is_absolute() {
        return Err("output must be an absolute new file path".into());
    }
    let parent = output.parent().ok_or("output has no directory")?;
    let parent = parent
        .canonicalize()
        .map_err(|_| "output directory unavailable")?;
    let name = output.file_name().ok_or("output has no file name")?;
    let output = parent.join(name);
    let mut pending = tempfile::NamedTempFile::new_in(&parent)
        .map_err(|_| "cannot stage manifest in output directory")?;
    pending
        .write_all(bytes)
        .map_err(|_| "cannot write staged manifest")?;
    pending
        .as_file()
        .sync_all()
        .map_err(|_| "cannot sync staged manifest")?;
    // Same-directory no-clobber publication: readers see only the complete verified JSON.
    // Dropping NamedTempFile cleans staging on every preceding failure.
    pending
        .persist_noclobber(&output)
        .map_err(|_| "output exists or publication failed")?;
    Ok(output)
}

/// Existing operator custody: reject symlinks atomically and never expose key bytes.
#[cfg(unix)]
pub(super) fn read_key(path: &Path) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    use rustix::fs::{open, Mode, OFlags};
    use std::os::unix::fs::MetadataExt;
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| "key unavailable or symlink")?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata().map_err(|_| "key metadata")?;
    if !metadata.is_file()
        || metadata.mode() & 0o7777 != 0o600
        || metadata.len() != 32
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err("key must be a regular non-symlink 32-byte seed with mode 0600".into());
    }
    let mut seed = zeroize::Zeroizing::new([0_u8; 32]);
    file.read_exact(&mut *seed).map_err(|_| "key unreadable")?;
    let mut extra = [0_u8; 1];
    if file.read(&mut extra).map_err(|_| "key unreadable")? != 0 {
        zeroize::Zeroize::zeroize(&mut extra);
        return Err("key changed size during read".into());
    }
    Ok(seed)
}

#[cfg(not(unix))]
pub(super) fn read_key(_path: &Path) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    Err("publisher key custody requires a Unix regular file with mode 0600".into())
}
