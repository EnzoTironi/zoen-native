//! Immutable historical checkpoint commitments. Later unrelated effects cannot
//! invalidate an already-consumed publication's exact original commitment.
use super::file::{options, regular, sync_directory, unavailable};
use super::{AnchorError, AnchorReader, Checkpoint, Prepared};
use std::fs::{self, File};
use std::io::{Read, Write};

impl AnchorReader {
    pub(super) fn checkpoint_effect(
        &self,
        checkpoint: &Checkpoint,
    ) -> Result<Option<Prepared>, AnchorError> {
        let path = self
            .private_directory("checkpoints")?
            .join(format!("{}.json", checkpoint.sequence));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(unavailable(error)),
            Ok(_) => {}
        }
        regular(&path)?;
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(unavailable)?
            .take(4097)
            .read_to_end(&mut bytes)
            .map_err(unavailable)?;
        if bytes.len() > 4096 {
            return Err(AnchorError::Invalid);
        }
        let prepared: Prepared =
            serde_json::from_slice(&bytes).map_err(|_| AnchorError::Invalid)?;
        if prepared.next != *checkpoint
            || super::state::transition(
                &self.deployment,
                &prepared.previous,
                &prepared.effect_digest,
                prepared.retirement,
            )
            .as_ref()
                != Some(&prepared)
        {
            return Err(AnchorError::Invalid);
        }
        Ok(Some(prepared))
    }

    pub(super) fn retain_checkpoint(&self, prepared: &Prepared) -> Result<(), AnchorError> {
        let directory = self.private_directory("checkpoints")?;
        let path = directory.join(format!("{}.json", prepared.next.sequence));
        let mut file = match options().open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if self.checkpoint_effect(&prepared.next)?.as_ref() == Some(prepared) {
                    return Ok(());
                }
                return Err(AnchorError::Invalid);
            }
            Err(error) => return Err(unavailable(error)),
        };
        // Never remove a partial record on failure. It is evidence of an
        // uncertain commit and cannot be mistaken for a never-used checkpoint.
        file.write_all(&serde_json::to_vec(prepared).map_err(|_| AnchorError::Invalid)?)
            .map_err(unavailable)?;
        file.sync_all().map_err(unavailable)?;
        sync_directory(&directory)
    }
}
