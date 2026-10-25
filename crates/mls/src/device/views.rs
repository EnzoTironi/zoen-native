//! Read the same verified group and checkpoint state without mutation.
use super::Device;
use crate::group::{identities, leaf_keys, LeafKeys, CHECKPOINT_TAG};
use crate::MlsError;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

impl Device<'_> {
    /// Member identities in the group as this device has it.
    pub fn roster(&self, space: &str) -> Result<BTreeSet<String>, MlsError> {
        self.with(|p| identities(&self.load(p, space)?))
    }

    /// The group's leaves: (identity, device) -> the leaf's current encryption key. A leaf
    /// that was taken out and added again (a device back from a long absence) has a new key.
    pub fn leaves(&self, space: &str) -> Result<LeafKeys, MlsError> {
        self.with(|p| leaf_keys(&self.load(p, space)?))
    }

    /// Verified leaf keys of a group with no commit awaiting confirmation. Missing
    /// groups and held recovery commits return `None`; storage failures remain errors.
    pub fn committed_leaves(&self, space: &str) -> Result<Option<LeafKeys>, MlsError> {
        if self.recovery_pending(space) {
            return Ok(None);
        }
        self.with(|p| {
            let group = match self.load(p, space) {
                Ok(group) => group,
                Err(MlsError::NoGroup) => return Ok(None),
                Err(error) => return Err(error),
            };
            if group.pending_commit().is_some() {
                return Ok(None);
            }
            leaf_keys(&group).map(Some)
        })
    }

    pub fn epoch(&self, space: &str) -> Result<u64, MlsError> {
        self.with(|p| Ok(self.load(p, space)?.epoch().as_u64()))
    }

    /// `(epoch, digest)` for a `Checkpoint`: SHA-256 over a tag, the group id, the epoch and
    /// the epoch authenticator. Only members of this exact epoch can compute it.
    pub fn checkpoint(&self, space: &str) -> Result<(u64, String), MlsError> {
        self.with(|p| {
            let group = self.load(p, space)?;
            let epoch = group.epoch().as_u64();
            let mut h = Sha256::new();
            h.update(CHECKPOINT_TAG);
            h.update(group.group_id().as_slice());
            h.update(epoch.to_be_bytes());
            h.update(group.epoch_authenticator().as_slice());
            Ok((epoch, hex::encode(h.finalize())))
        })
    }
}
