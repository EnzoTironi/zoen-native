use super::AnchorError;
use std::fs::{File, TryLockError};

/// Authority belongs to this guard, not to descriptors inherited by a child
/// between fork and exec. Closing alone can leave Linux flock held by that child.
pub(super) struct ScopedLock {
    file: File,
    held: bool,
}

impl ScopedLock {
    pub(super) fn exclusive(file: File) -> Result<Self, AnchorError> {
        file.try_lock().map_err(|error| lock_error(&error))?;
        Ok(Self { file, held: true })
    }

    pub(super) fn shared(file: File) -> Result<Self, AnchorError> {
        file.try_lock_shared().map_err(|error| lock_error(&error))?;
        Ok(Self { file, held: true })
    }

    pub(super) fn unlock(&mut self) -> Result<(), AnchorError> {
        #[cfg(test)]
        super::faults::check(super::faults::Point::Unlock)?;
        self.file.unlock().map_err(super::file::unavailable)?;
        self.held = false;
        Ok(())
    }
}

impl Drop for ScopedLock {
    fn drop(&mut self) {
        if self.held {
            // Drop cannot report an I/O error. Explicit retirement release does,
            // retaining ownership for a normal same-writer reconciliation retry.
            let _ = self.unlock();
        }
    }
}

fn lock_error(error: &TryLockError) -> AnchorError {
    match error {
        TryLockError::WouldBlock => AnchorError::Busy,
        TryLockError::Error(_) => AnchorError::Unavailable,
    }
}
