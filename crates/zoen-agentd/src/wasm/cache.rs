//! Official Wasmtime compilation cache, inside an explicit operator trust boundary.
//! Compiled entries contain native code: this does not authenticate hostile cache
//! contents or impose a hard decompression/disk quota. No guest receives this path.
use super::WasmError;
use std::path::{Path, PathBuf};
use wasmtime::{Cache, CacheConfig};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct CompilationCache {
    directory: PathBuf,
    identity: (u64, u64),
    official: Cache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompilationCacheStats {
    pub hits: usize,
    pub misses: usize,
}

impl CompilationCache {
    /// Opens an existing, private operator-owned directory. Does not create or
    /// adopt a directory, read a user/global Wasmtime config, or load artifacts.
    /// Unix ownership validation is required; other platforms explicitly refuse.
    pub fn private_directory(directory: &Path) -> Result<Self, WasmError> {
        let identity = validate(directory)?;
        let mut config = CacheConfig::new();
        config
            .with_directory(directory)
            .with_file_count_soft_limit(32)
            .with_files_total_size_soft_limit(512 * 1024 * 1024);
        let official = Cache::new(config)
            .map_err(|_| WasmError::CacheConfiguration("Wasmtime rejected cache configuration"))?;
        Ok(Self {
            directory: directory.to_path_buf(),
            identity,
            official,
        })
    }

    pub fn stats(&self) -> CompilationCacheStats {
        CompilationCacheStats {
            hits: self.official.cache_hits(),
            misses: self.official.cache_misses(),
        }
    }

    pub(super) fn verified_official(&self) -> Result<Cache, WasmError> {
        self.check()?;
        Ok(self.official.clone())
    }

    pub(super) fn check(&self) -> Result<(), WasmError> {
        if validate(&self.directory)? != self.identity {
            return Err(WasmError::CacheConfiguration(
                "cache directory was replaced",
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn validate(directory: &Path) -> Result<(u64, u64), WasmError> {
    use std::os::unix::fs::MetadataExt;
    use std::path::Component;
    let bad = |reason| WasmError::CacheConfiguration(reason);
    if !directory.is_absolute()
        || directory
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(bad("an absolute normalized directory is required"));
    }
    let uid = rustix::process::geteuid().as_raw();
    let mut identity = None;
    for (index, path) in directory.ancestors().enumerate() {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| bad("cache directory and parents must already exist"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(bad(
                "cache directory and parents must be directories without symlinks",
            ));
        }
        if index == 0 {
            if metadata.uid() != uid || metadata.mode() & 0o777 != 0o700 {
                return Err(bad(
                    "cache directory must be owned by this user with mode 0700",
                ));
            }
            identity = Some((metadata.dev(), metadata.ino()));
        } else {
            let sticky_system_directory = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
            if (metadata.uid() != 0 && metadata.uid() != uid)
                || (metadata.mode() & 0o022 != 0 && !sticky_system_directory)
            {
                return Err(bad("cache ancestors must not permit untrusted replacement"));
            }
        }
    }
    identity.ok_or_else(|| bad("cache directory is unavailable"))
}

#[cfg(not(unix))]
fn validate(_directory: &Path) -> Result<(u64, u64), WasmError> {
    Err(WasmError::CacheConfiguration(
        "private cache ownership is unsupported on this platform",
    ))
}
