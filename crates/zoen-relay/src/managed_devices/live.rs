//! Append-only live operator configuration; no request chooses its file/source.
use super::ManagedDevices;
#[cfg(test)]
mod tests;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Stamp {
    modified: SystemTime,
    length: u64,
    #[cfg(unix)]
    identity: (u64, u64, u32),
}
impl Stamp {
    pub(super) fn read(path: &Path) -> anyhow::Result<Self> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
        anyhow::ensure!(metadata.is_file(), "invalid live operator file");
        Ok(Self {
            modified: metadata.modified().map_err(|_| invalid())?,
            length: metadata.len(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino(), metadata.mode())
            },
        })
    }
}
fn invalid() -> anyhow::Error {
    anyhow::anyhow!("live managed admission unavailable")
}

pub(super) struct Source {
    path: PathBuf,
    observed: Mutex<Stamp>,
    source: OnceLock<String>,
}
impl Source {
    pub(super) fn new(path: &Path, observed: Stamp) -> Self {
        Self {
            path: path.into(),
            observed: Mutex::new(observed),
            source: OnceLock::new(),
        }
    }
    pub(super) fn bind_source(&self, cell: &str) -> bool {
        self.source.get_or_init(|| cell.to_owned()) == cell
    }
    pub(super) fn refresh(&self, registry: &ManagedDevices) -> anyhow::Result<()> {
        let cell = self.source.get().ok_or_else(invalid)?;
        let mut observed = self.observed.lock().map_err(|_| invalid())?;
        let next_stamp = Stamp::read(&self.path)?;
        if next_stamp == *observed {
            return Ok(());
        }
        let candidate = ManagedDevices::load_operator_file(Some(&self.path))?;
        anyhow::ensure!(
            candidate.live.is_some()
                && candidate.source_matches(cell)
                && Stamp::read(&self.path)? == next_stamp,
            "invalid live managed admission"
        );
        let next = candidate.snapshot().clone();
        let mut current = match registry.devices.write() {
            Ok(map) => map,
            Err(poisoned) => poisoned.into_inner(),
        };
        // Neither deletion, downgrade, reassignment nor anchor relocation is a
        // live enrollment. Existing devices remain managed even on bad input.
        anyhow::ensure!(
            current.iter().all(|(device, entry)| next
                .get(device)
                .is_some_and(|new| entry.deployment == new.deployment
                    && entry.anchor.same_configuration(&new.anchor))),
            "existing managed admission cannot change"
        );
        *current = next;
        *observed = next_stamp;
        Ok(())
    }
}
