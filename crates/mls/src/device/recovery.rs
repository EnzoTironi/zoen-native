//! Recovery staging and durable reconciliation markers for this device.
use super::Device;
use crate::group::{
    capabilities, commit_bytes, group_id, identities, join_config, unlisted, Commit, Opened,
};
use crate::{mls, MlsError, SUITE};
use openmls::messages::group_info::VerifiableGroupInfo;
use openmls::prelude::{
    tls_codec::Deserialize as _, LeafNodeParameters, MlsGroup, OpenMlsProvider as _,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

impl Device<'_> {
    /// Join from the latest authenticated public context without an online member.
    /// The caller must sequence the returned external commit before confirming it.
    /// # Errors
    /// Refuses unlisted identities, invalid public context, existing group, or failed external commit staging/storage.
    pub fn recover(
        &self,
        space: &str,
        group_info: &[u8],
        roster: &BTreeSet<String>,
    ) -> Result<Commit, MlsError> {
        if !roster.contains(&self.leaf.identity) {
            return Err(MlsError::Unlisted(BTreeSet::from([self
                .leaf
                .identity
                .clone()])));
        }
        let verified = crate::recovery::verify_group_info(space, group_info, None)?;
        unlisted(verified.identities, roster)?;
        let info = VerifiableGroupInfo::tls_deserialize_exact(group_info)
            .map_err(|e| MlsError::Decode(e.to_string()))?;
        if info.group_id() != &group_id(space) || info.ciphersuite() != SUITE {
            return Err(MlsError::Mls("GroupInfo for another group or suite".into()));
        }
        // The native engine wraps group state, outbox and blob queue together. A direct
        // caller still gets rollback on every validation or storage error.
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction().map_err(mls))
            .transpose()?;
        let out = self.with(|p| {
            if Self::load(p, space).is_ok() {
                return Err(MlsError::Mls("this device already has a group".into()));
            }
            let (group, bundle) = MlsGroup::external_commit_builder()
                .with_config(join_config())
                .build_group(p, info, self.credential())
                .map_err(mls)?
                .leaf_node_parameters(
                    LeafNodeParameters::builder()
                        .with_capabilities(capabilities())
                        .build(),
                )
                .load_psks(p.storage())
                .map_err(mls)?
                .build(p.rand(), p.crypto(), &self.signer, |_| true)
                .map_err(mls)?
                .finalize(p)
                .map_err(mls)?;
            unlisted(identities(&group)?, roster)?;
            let c = commit_bytes(&bundle)?;
            self.conn
                .execute(
                    "INSERT INTO mls_recovery_pending (space, commit_hash) VALUES (?1, ?2)",
                    rusqlite::params![space, Sha256::digest(&c.commit).as_slice()],
                )
                .map_err(mls)?;
            Ok(c)
        })?;
        if let Some(tx) = tx {
            tx.commit().map_err(mls)?;
        }
        Ok(out)
    }

    /// Only after the log confirms this device's exact external commit.
    /// # Errors
    /// Refuses a different pending recovery commit and failed state/reconciliation reads or transaction writes.
    pub fn confirm_recovery(
        &self,
        space: &str,
        commit: &[u8],
        roster: &BTreeSet<String>,
    ) -> Result<Opened, MlsError> {
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()
            .map_err(|e| MlsError::Storage(e.to_string()))?;
        let opened = self.with(|p| {
            let expected: Vec<u8> = self
                .conn
                .query_row(
                    "SELECT commit_hash FROM mls_recovery_pending WHERE space = ?1",
                    [space],
                    |r| r.get(0),
                )
                .map_err(|e| MlsError::Storage(e.to_string()))?;
            if expected != Sha256::digest(commit).as_slice() {
                return Err(MlsError::Mls(
                    "another recovery commit was confirmed".into(),
                ));
            }
            let group = Self::load(p, space)?;
            // Staging verified this roster. A later clear removal cannot leave
            // the only confirmed handshake attached to an unsequenced marker.
            let reconcile = !identities(&group)?.is_subset(roster);
            self.set_reconciliation(space, reconcile)?;
            self.conn
                .execute("DELETE FROM mls_recovery_pending WHERE space = ?1", [space])
                .map_err(|e| MlsError::Storage(e.to_string()))?;
            let epoch = group.epoch().as_u64();
            Ok(if reconcile {
                Opened::Reconcile { epoch }
            } else {
                Opened::Commit { epoch }
            })
        })?;
        if let Some(tx) = tx {
            tx.commit().map_err(|e| MlsError::Storage(e.to_string()))?;
        }
        Ok(opened)
    }

    pub(super) fn set_reconciliation(&self, space: &str, held: bool) -> Result<(), MlsError> {
        let sql = if held {
            "INSERT INTO mls_recovery_reconcile (space) VALUES (?1) ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM mls_recovery_reconcile WHERE space = ?1"
        };
        self.conn
            .execute(sql, [space])
            .map(drop)
            .map_err(|e| MlsError::Storage(e.to_string()))
    }

    #[must_use]
    pub fn needs_reconciliation(&self, space: &str) -> bool {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM mls_recovery_reconcile WHERE space = ?1)",
                [space],
                |r| r.get(0),
            )
            .unwrap_or(true)
    }

    /// An external commit that lost its epoch must discard its tentative group.
    /// # Errors
    /// Returns failures deleting the tentative group and its recovery markers.
    pub fn abandon_recovery(&self, space: &str) -> Result<(), MlsError> {
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction().map_err(mls))
            .transpose()?;
        self.with(|p| {
            Self::load(p, space)?.delete(p.storage()).map_err(mls)?;
            self.conn
                .execute("DELETE FROM mls_recovery_pending WHERE space = ?1", [space])
                .map(drop)
                .map_err(mls)?;
            self.set_reconciliation(space, false)
        })?;
        if let Some(tx) = tx {
            tx.commit().map_err(mls)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn recovery_pending(&self, space: &str) -> bool {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM mls_recovery_pending WHERE space = ?1)",
                [space],
                |r| r.get(0),
            )
            .unwrap_or(true)
    }
}
