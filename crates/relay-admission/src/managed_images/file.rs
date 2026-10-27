use super::lock::ScopedLock;
use super::state::State;
use super::{AnchorError, Checkpoint, Deployment, Prepared, Recovery};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writer ownership lasts until the last in-flight host reference is dropped.
/// Lock filenames must never be unlinked or replaced, including after crashes.
pub struct AnchorWriter {
    reader: AnchorReader,
    _writer: ScopedLock,
    retirement: Option<ScopedLock>,
}

#[derive(Clone)]
pub struct AnchorReader {
    pub(super) directory: PathBuf,
    pub(super) deployment: Deployment,
}

/// Relay/gateway effects retain this shared OS lock through the actual effect.
/// It is deliberately neither Clone nor Deserialize. Retirement takes exclusive.
pub struct GenerationGuard {
    _lock: ScopedLock,
}

pub(super) fn unavailable(_: std::io::Error) -> AnchorError {
    AnchorError::Unavailable
}
pub(super) fn regular(path: &Path) -> Result<(), AnchorError> {
    let metadata = fs::symlink_metadata(path).map_err(unavailable)?;
    if !metadata.is_file() {
        return Err(AnchorError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AnchorError::Invalid);
        }
    }
    Ok(())
}
fn lock_file(directory: &Path, name: &str) -> Result<File, AnchorError> {
    let path = directory.join(name);
    regular(&path)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(unavailable)
}
pub(super) fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
pub(super) fn sync_directory(path: &Path) -> Result<(), AnchorError> {
    // Do not claim crash durability on platforms lacking directory fsync.
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(unavailable)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(AnchorError::Unavailable)
    }
}

impl AnchorReader {
    /// # Errors
    /// Refuses invalid/inaccessible private state or deployment mismatch; writer opens also refuse an occupied writer lock.
    pub fn open(directory: &Path, deployment: Deployment) -> Result<Self, AnchorError> {
        if !deployment.valid()
            || !fs::symlink_metadata(directory)
                .map_err(unavailable)?
                .is_dir()
        {
            return Err(AnchorError::Invalid);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::symlink_metadata(directory)
                .map_err(unavailable)?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err(AnchorError::Invalid);
            }
        }
        let reader = Self {
            directory: directory.into(),
            deployment,
        };
        reader.state()?;
        Ok(reader)
    }

    pub(super) fn state(&self) -> Result<State, AnchorError> {
        self.effect_directory()?;
        self.private_directory("checkpoints")?;
        let path = self.directory.join("checkpoint.json");
        regular(&path)?;
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(unavailable)?
            .take(16_385)
            .read_to_end(&mut bytes)
            .map_err(unavailable)?;
        if bytes.len() > 16_384 {
            return Err(AnchorError::Invalid);
        }
        let state: State = serde_json::from_slice(&bytes).map_err(|_| AnchorError::Invalid)?;
        if !state.valid() || state.deployment != self.deployment {
            return Err(AnchorError::Invalid);
        }
        if let Some(committed) = &state.committed_effect {
            if self.checkpoint_effect(&state.committed)?.as_ref() != Some(committed) {
                return Err(AnchorError::Unreconciled);
            }
        }
        Ok(state)
    }

    pub(super) fn effect_directory(&self) -> Result<PathBuf, AnchorError> {
        self.private_directory("effects")
    }

    pub(super) fn private_directory(&self, name: &str) -> Result<PathBuf, AnchorError> {
        let directory = self.directory.join(name);
        let metadata = fs::symlink_metadata(&directory).map_err(unavailable)?;
        if !metadata.is_dir() {
            return Err(AnchorError::Invalid);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(AnchorError::Invalid);
            }
        }
        Ok(directory)
    }

    /// # Errors
    /// Refuses invalid/unavailable state or a still-prepared transition.
    pub fn checkpoint(&self) -> Result<Checkpoint, AnchorError> {
        let state = self.state()?;
        if state.prepared.is_some() {
            return Err(AnchorError::Unreconciled);
        }
        Ok(state.committed)
    }

    /// # Errors
    /// Refuses an occupied generation lock, prepared state or a checkpoint other than the committed tip.
    pub fn generation_guard(&self, expected: &Checkpoint) -> Result<GenerationGuard, AnchorError> {
        let lock = ScopedLock::shared(lock_file(&self.directory, "generation.lock")?)?;
        let state = self.state()?;
        if state.prepared.is_some() || state.committed != *expected {
            return Err(AnchorError::Unreconciled);
        }
        Ok(GenerationGuard { _lock: lock })
    }

    /// Hold the generation of an exact retained effect while unrelated effects
    /// advance the tip. This is not a publication permit or a store checkpoint
    /// attestation; the caller must also validate its actual current store read.
    /// # Errors
    /// Refuses unavailable/invalid state, a different generation or a missing retained effect checkpoint.
    pub fn historical_generation_guard(
        &self,
        expected: &Checkpoint,
    ) -> Result<GenerationGuard, AnchorError> {
        self.effect_generation_guard(expected)
    }

    pub(super) fn effect_generation_guard(
        &self,
        expected: &Checkpoint,
    ) -> Result<GenerationGuard, AnchorError> {
        let lock = ScopedLock::shared(lock_file(&self.directory, "generation.lock")?)?;
        let state = self.state()?;
        if state.committed.generation != expected.generation
            || state.committed.sequence < expected.sequence
            || self.checkpoint_effect(expected)?.is_none()
        {
            return Err(AnchorError::Unreconciled);
        }
        Ok(GenerationGuard { _lock: lock })
    }
}

impl AnchorWriter {
    /// Only a freshly created owned directory can bootstrap. The Runtime must
    /// independently prove an unused namespace before invoking this method.
    /// Missing state in an existing directory is never adopted or regenerated.
    /// # Errors
    /// Refuses invalid deployment, pre-existing directories and any failed durable directory/file/state initialization.
    pub fn create(directory: &Path, deployment: Deployment) -> Result<Self, AnchorError> {
        let state = State::initial(deployment.clone()).ok_or(AnchorError::Invalid)?;
        let parent = directory.parent().ok_or(AnchorError::Invalid)?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory).map_err(unavailable)?;
        sync_directory(parent)?;
        builder
            .create(directory.join("effects"))
            .map_err(unavailable)?;
        sync_directory(&directory.join("effects"))?;
        builder
            .create(directory.join("checkpoints"))
            .map_err(unavailable)?;
        sync_directory(&directory.join("checkpoints"))?;
        for name in ["writer.lock", "generation.lock"] {
            options()
                .open(directory.join(name))
                .and_then(|file| file.sync_all())
                .map_err(unavailable)?;
        }
        let reader = AnchorReader {
            directory: directory.into(),
            deployment,
        };
        write(&reader.directory, &state)?;
        Self::open(directory, reader.deployment)
    }

    /// # Errors
    /// Refuses invalid/inaccessible private state or deployment mismatch; writer opens also refuse an occupied writer lock.
    pub fn open(directory: &Path, deployment: Deployment) -> Result<Self, AnchorError> {
        let reader = AnchorReader::open(directory, deployment)?;
        let writer = ScopedLock::exclusive(lock_file(directory, "writer.lock")?)?;
        Ok(Self {
            reader,
            _writer: writer,
            retirement: None,
        })
    }

    #[must_use]
    pub fn reader(&self) -> AnchorReader {
        self.reader.clone()
    }

    /// # Errors
    /// Refuses unavailable or invalid authoritative anchor state.
    pub fn pending(&self) -> Result<Option<Prepared>, AnchorError> {
        Ok(self.reader.state()?.prepared)
    }

    /// # Errors
    /// Refuses unreconciled store observations, another prepared effect, invalid transition or failed generation lock/durable write.
    pub fn prepare(
        &mut self,
        effect: &str,
        sql: &Checkpoint,
        fdb: &Checkpoint,
        retirement: bool,
    ) -> Result<Prepared, AnchorError> {
        let mut state = self.reader.state()?;
        if state.prepared.is_some() || state.recovery(sql, fdb) != Some(Recovery::Clean) {
            return Err(AnchorError::Unreconciled);
        }
        let prepared = state
            .prepare(effect, retirement)
            .ok_or(AnchorError::Invalid)?;
        if retirement {
            let lock =
                ScopedLock::exclusive(lock_file(&self.reader.directory, "generation.lock")?)?;
            self.retirement = Some(lock);
        }
        state.prepared = Some(prepared.clone());
        write(&self.reader.directory, &state)?;
        Ok(prepared)
    }

    /// Reconcile actual store observations. Partial states stay PREPARED until
    /// the Runtime verifies/reconciles the original claim/admission; this method
    /// neither overwrites a store nor fabricates a provider/relay permission.
    /// # Errors
    /// Refuses unknown partial states, inconsistent history, unavailable locks or failed durable writes/releases; never fabricates a store commit.
    pub fn reconcile(
        &mut self,
        sql: &Checkpoint,
        fdb: &Checkpoint,
    ) -> Result<Recovery, AnchorError> {
        let mut state = self.reader.state()?;
        let result = state.recovery(sql, fdb).ok_or(AnchorError::Unreconciled)?;
        if state
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.retirement)
            && self.retirement.is_none()
        {
            let lock =
                ScopedLock::exclusive(lock_file(&self.reader.directory, "generation.lock")?)?;
            self.retirement = Some(lock);
        }
        if matches!(result, Recovery::BothBefore | Recovery::BothAfter) {
            let prepared = state.prepared.take().ok_or(AnchorError::Invalid)?;
            if result == Recovery::BothBefore
                && self.reader.checkpoint_effect(&prepared.next)?.is_some()
            {
                return Err(AnchorError::Unreconciled);
            }
            if result == Recovery::BothAfter {
                self.reader.retain_checkpoint(&prepared)?;
                state.committed = prepared.next.clone();
                state.committed_effect = Some(prepared);
            }
            write(&self.reader.directory, &state)?;
        }
        if matches!(
            result,
            Recovery::Clean | Recovery::BothBefore | Recovery::BothAfter
        ) {
            // A prior rename may have committed even when its directory fsync or
            // unlock acknowledgement failed. Re-establish durability on Clean
            // before releasing retained ownership; never unlock a partial state.
            if let Some(lock) = self.retirement.as_mut() {
                if result == Recovery::Clean {
                    #[cfg(test)]
                    super::faults::check(super::faults::Point::DirectorySync)?;
                    sync_directory(&self.reader.directory)?;
                }
                lock.unlock()?;
            }
            self.retirement = None;
        }
        Ok(result)
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

fn write(directory: &Path, state: &State) -> Result<(), AnchorError> {
    if !state.valid() {
        return Err(AnchorError::Invalid);
    }
    let bytes = serde_json::to_vec(state).map_err(|_| AnchorError::Invalid)?;
    let mut selected = None;
    for _ in 0..128 {
        let path = directory.join(format!(
            ".checkpoint-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match options().open(&path) {
            Ok(file) => {
                selected = Some((file, Temporary(path)));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(unavailable(error)),
        }
    }
    let (mut file, temporary) = selected.ok_or(AnchorError::Unavailable)?;
    #[cfg(test)]
    super::faults::check(super::faults::Point::Write)?;
    file.write_all(&bytes).map_err(unavailable)?;
    #[cfg(test)]
    super::faults::check(super::faults::Point::FileSync)?;
    file.sync_all().map_err(unavailable)?;
    drop(file);
    fs::rename(&temporary.0, directory.join("checkpoint.json")).map_err(unavailable)?;
    #[cfg(test)]
    super::faults::check(super::faults::Point::DirectorySync)?;
    // A failed acknowledgement after rename never rolls back or deletes state.
    sync_directory(directory)
}
