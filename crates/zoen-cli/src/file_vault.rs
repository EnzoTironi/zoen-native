//! Local CLI secrets. Each replacement is atomic; a failed acknowledgement may
//! already have committed the complete new value. This is not a multi-key
//! transaction or a substitute for the enrollment/setup recovery record. The
//! CLI establishes its home before opening this adapter; only the vault's entry
//! within that existing home is created and synchronized here.
use client_core::SecretVault;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const TEMP_PREFIX: &str = ".vault-write.";

pub(crate) struct FileVault {
    pub(crate) dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveStage {
    DirectoryReady,
    TemporaryCreated,
    BytesWritten,
    FileSynced,
    Renamed,
    DirectorySynced,
}

struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            // Only this writer's uncommitted temporary file. Orphans left by
            // process death are never mistaken for keys or deleted on startup.
            let _ = fs::remove_file(path);
        }
    }
}

fn invalid_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid local vault path")
}

fn validate_key(key: &str) -> io::Result<()> {
    if key.is_empty()
        || key.len() > 128
        || key == "."
        || key == ".."
        || key.starts_with(TEMP_PREFIX)
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid_path());
    }
    Ok(())
}

fn require_regular_or_missing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_path()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        // std does not expose a portable directory-flush primitive. File sync
        // and atomic replacement still apply; directory durability is currently
        // qualified only on the tested Unix host, not on Windows/network shares.
        let _ = path;
        Ok(())
    }
}

impl FileVault {
    pub(crate) fn read_bytes(&self, key: &str) -> io::Result<Vec<u8>> {
        validate_key(key)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        fs::read(path)
    }

    pub(crate) fn write_bytes(&self, key: &str, value: &[u8]) -> io::Result<()> {
        self.write_observed(key, value, |_| Ok(()))
    }

    // The private observer lets tests interrupt the actual filesystem path at
    // commit boundaries. Production has only the no-op observer above.
    fn write_observed(
        &self,
        key: &str,
        value: &[u8],
        mut after: impl FnMut(SaveStage) -> io::Result<()>,
    ) -> io::Result<()> {
        validate_key(key)?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&self.dir) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
        if !fs::symlink_metadata(&self.dir)?.is_dir() {
            return Err(invalid_path());
        }
        // Sync even when the vault exists: its creation may have been interrupted
        // before this acknowledgement. Do not require access to unrelated system
        // ancestors; establishing the parent home belongs to the CLI caller.
        let parent = self
            .dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        sync_directory(parent)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        after(SaveStage::DirectoryReady)?;

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let (mut file, temporary) = (0..128)
            .find_map(|_| {
                let temporary = self.dir.join(format!(
                    "{TEMP_PREFIX}{}.{timestamp}.{}.tmp",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match options.open(&temporary) {
                    Ok(file) => Some(Ok((file, temporary))),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .unwrap_or_else(|| Err(io::Error::other("local vault temporary path unavailable")))?;
        let mut guard = Temporary(Some(temporary.clone()));
        after(SaveStage::TemporaryCreated)?;
        file.write_all(value)?;
        after(SaveStage::BytesWritten)?;
        file.sync_all()?;
        after(SaveStage::FileSynced)?;
        drop(file);
        require_regular_or_missing(&path)?;
        fs::rename(&temporary, &path)?;
        guard.0 = None;
        // Do not roll back or delete the destination on any error after rename.
        // A caller receiving false must reconcile the complete retained value.
        after(SaveStage::Renamed)?;
        sync_directory(&self.dir)?;
        after(SaveStage::DirectorySynced)?;
        Ok(())
    }

    pub(crate) fn remove(&self, key: &str) -> io::Result<()> {
        validate_key(key)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        fs::remove_file(path)?;
        sync_directory(&self.dir)
    }
}

#[cfg(test)]
#[path = "file_vault_tests.rs"]
mod tests;

impl SecretVault for FileVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.read_bytes(&key).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.write_bytes(&key, &value).is_ok()
    }
    fn delete(&self, key: String) {
        // SecretVault cannot report deletion errors. Callers needing confirmed
        // removal must still reconcile with the retained enrollment/setup state.
        let _ = self.remove(&key);
    }
}
