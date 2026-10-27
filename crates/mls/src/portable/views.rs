//! Verified views and explicit forgetting over the same host-owned snapshot.
use super::{ExternalDevice, Snapshot};
use crate::group::{identities, CHECKPOINT_TAG};
use crate::{mls, MlsError};
use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::signatures::Signer;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

impl<P, S> ExternalDevice<'_, P, S>
where
    P: OpenMlsProvider<CryptoProvider = RustCrypto, RandProvider = RustCrypto>,
    S: Signer,
{
    /// # Errors
    /// Propagates snapshot read and group-state deletion failures.
    pub fn forget(&self, space: &str) -> Result<(), MlsError> {
        match self.snapshot(space)? {
            Snapshot::Absent => Ok(()),
            Snapshot::Stale(group) => self.drop_stored(space, group),
            Snapshot::Ours(group) | Snapshot::Foreign(group) => {
                self.drop_stored(space, Some(group))
            }
        }
    }
    /// # Errors
    /// Refuses absent/foreign groups, invalid credentials or failed state reads.
    pub fn roster(&self, space: &str) -> Result<BTreeSet<String>, MlsError> {
        identities(&self.load(space)?)
    }
    /// Current verified device encryption keys, for shared membership reconciliation.
    /// # Errors
    /// Refuses absent/foreign groups, invalid credentials or failed state reads.
    pub fn leaves(&self, space: &str) -> Result<crate::LeafKeys, MlsError> {
        crate::group::leaf_keys(&self.load(space)?)
    }
    /// An unconfirmed commit must finish or be abandoned before another is planned.
    /// # Errors
    /// Refuses foreign credentials or failed state reads; absent/stale and pending groups return None.
    pub fn committed_leaves(&self, space: &str) -> Result<Option<crate::LeafKeys>, MlsError> {
        let group = match self.snapshot(space)? {
            Snapshot::Absent | Snapshot::Stale(_) => return Ok(None),
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Ours(group) => group,
        };
        if group.pending_commit().is_some() {
            return Ok(None);
        }
        crate::group::leaf_keys(&group).map(Some)
    }
    /// Discard only a staged commit that lost the relay epoch race.
    /// # Errors
    /// Propagates group loading and pending-commit storage failures.
    pub fn abandon(&self, space: &str) -> Result<(), MlsError> {
        let mut group = self.load(space)?;
        group
            .clear_pending_commit(self.provider.storage())
            .map_err(mls)
    }
    /// # Errors
    /// Propagates group loading failures, including absent/foreign snapshots.
    pub fn epoch(&self, space: &str) -> Result<u64, MlsError> {
        Ok(self.load(space)?.epoch().as_u64())
    }
    /// # Errors
    /// Propagates group loading failures, including absent/foreign snapshots.
    pub fn checkpoint(&self, space: &str) -> Result<(u64, String), MlsError> {
        let group = self.load(space)?;
        let epoch = group.epoch().as_u64();
        let digest = Sha256::new()
            .chain_update(CHECKPOINT_TAG)
            .chain_update(group.group_id().as_slice())
            .chain_update(epoch.to_be_bytes())
            .chain_update(group.epoch_authenticator().as_slice())
            .finalize();
        Ok((epoch, hex::encode(digest)))
    }
}
