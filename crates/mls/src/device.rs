//! The device half: groups, sealing and opening, with state in the device's SQLite.

use std::collections::BTreeSet;

use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::RustCrypto;
use openmls_sqlite_storage::SqliteStorageProvider;
use rusqlite::Connection;

use crate::group::{
    capabilities, commit_bytes, create_config, group_id, join_config, storage_error, unlisted,
    welcome_error, Commit, GroupState,
};
use crate::sealed::{Keys, Scope, SealedCodec};
use crate::{leaf_name, leaf_of, mls, validate_key_package, Leaf, MlsError, SUITE};

mod opening;
mod recovery;
mod views;

struct Provider<'c> {
    crypto: RustCrypto,
    storage: SqliteStorageProvider<SealedCodec, &'c Connection>,
}

impl<'c> OpenMlsProvider for Provider<'c> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqliteStorageProvider<SealedCodec, &'c Connection>;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }
    fn crypto(&self) -> &RustCrypto {
        &self.crypto
    }
    fn rand(&self) -> &RustCrypto {
        &self.crypto
    }
}

/// Creates the MLS tables in the device database. Idempotent; run at every open.
/// # Errors
/// Returns an error if MLS table migrations or recovery-table creation fail.
pub fn migrate(conn: &mut Connection) -> Result<(), MlsError> {
    SqliteStorageProvider::<SealedCodec, &mut Connection>::new(conn)
        .run_migrations()
        .map_err(mls)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mls_recovery_pending (
            space TEXT PRIMARY KEY, commit_hash BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS mls_recovery_reconcile (
            space TEXT PRIMARY KEY
         );",
    )
    .map_err(mls)
}

/// Device-local recovery state must be erased with its outbox. The engine
/// owns that transaction; this does not erase unrelated pre-existing MLS data.
/// # Errors
/// Returns a storage error if recovery markers cannot be deleted in the caller transaction.
pub fn clear_recovery_markers(conn: &Connection) -> Result<(), MlsError> {
    conn.execute_batch("DELETE FROM mls_recovery_pending; DELETE FROM mls_recovery_reconcile;")
        .map_err(|e| MlsError::Storage(e.to_string()))
}

/// This device as an MLS client, over the device database.
pub struct Device<'c> {
    conn: &'c Connection,
    state_keys: Keys,
    signer: SignatureKeyPair,
    leaf: Leaf,
}

impl<'c> Device<'c> {
    /// `device_secret` is the device's Ed25519 key (the same one that signs envelopes),
    /// `cert` the identity's certificate over it, `state_key` the vault key that seals state.
    /// # Errors
    /// Refuses invalid certified device credentials or failed state-key derivation.
    pub fn new(
        conn: &'c Connection,
        state_key: [u8; 32],
        identity: &str,
        device_secret: [u8; 32],
        cert: &str,
    ) -> Result<Self, MlsError> {
        let device = event_log::Signer::from_secret(&device_secret).id();
        let public = hex::decode(&device).map_err(mls)?;
        let leaf = Leaf {
            identity: identity.to_string(),
            device,
            cert: cert.to_string(),
        };
        if Leaf::verified(&leaf.encode(), &public).is_none() {
            return Err(MlsError::Credential);
        }
        let state_keys = Keys::derive(state_key).map_err(storage_error)?;
        Ok(Self {
            conn,
            state_keys,
            signer: SignatureKeyPair::from_raw(
                SUITE.signature_algorithm(),
                device_secret.to_vec(),
                public,
            ),
            leaf,
        })
    }

    #[must_use]
    pub fn leaf(&self) -> &Leaf {
        &self.leaf
    }

    /// Runs `f` with a provider over the device database and the state key in scope.
    fn with<R>(&self, f: impl FnOnce(&Provider<'c>) -> R) -> R {
        let _scope = Scope::enter(self.state_keys);
        f(&Provider {
            crypto: RustCrypto::default(),
            storage: SqliteStorageProvider::new(self.conn),
        })
    }

    fn credential(&self) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(self.leaf.encode()).into(),
            signature_key: self.signer.public().into(),
        }
    }

    fn load(p: &Provider<'c>, space: &str) -> Result<MlsGroup, MlsError> {
        MlsGroup::load(p.storage(), &group_id(space))
            .map_err(storage_error)?
            .ok_or(MlsError::NoGroup)
    }

    /// `count` single-use key packages, plus one last-resort package when asked: the one a
    /// claim falls back to when the others are gone. Their private keys stay sealed here.
    /// # Errors
    /// Returns key-package generation, storage, signing or encoding failures.
    pub fn key_packages(&self, count: usize, last_resort: bool) -> Result<Vec<Vec<u8>>, MlsError> {
        self.with(|p| {
            (0..count + usize::from(last_resort))
                .map(|i| {
                    let mut b = KeyPackage::builder().leaf_node_capabilities(capabilities());
                    if last_resort && i == count {
                        b = b.mark_as_last_resort();
                    }
                    let bundle = b
                        .build(SUITE, p, &self.signer, self.credential())
                        .map_err(mls)?;
                    bundle.key_package().tls_serialize_detached().map_err(mls)
                })
                .collect()
        })
    }

    #[must_use]
    pub fn has_group(&self, space: &str) -> bool {
        self.with(|p| Self::load(p, space).is_ok())
    }

    /// Reads the current group once. Storage failures never become missing groups.
    /// # Errors
    /// Returns storage failures rather than treating them as missing groups.
    pub fn group_state(&self, space: &str) -> Result<GroupState, MlsError> {
        if self.recovery_pending(space) {
            return Ok(GroupState::Pending);
        }
        self.with(|p| match Self::load(p, space) {
            Ok(group) if group.pending_commit().is_some() => Ok(GroupState::Pending),
            Ok(_) => Ok(GroupState::Ready),
            Err(MlsError::NoGroup) => Ok(GroupState::Missing),
            Err(error) => Err(error),
        })
    }

    /// Deletes this device's state for a group it was removed from: its secrets go, and
    /// a later Welcome (if it is added back) starts it afresh.
    /// # Errors
    /// Returns failed recovery-state cleanup or transaction errors.
    pub fn forget(&self, space: &str) -> Result<(), MlsError> {
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction().map_err(mls))
            .transpose()?;
        self.with(|p| match Self::load(p, space) {
            Ok(mut group) => group.delete(p.storage()).map_err(mls),
            Err(_) => Ok(()),
        })?;
        self.conn
            .execute("DELETE FROM mls_recovery_pending WHERE space = ?1", [space])
            .map_err(mls)?;
        self.set_reconciliation(space, false)?;
        if let Some(tx) = tx {
            tx.commit().map_err(mls)?;
        }
        Ok(())
    }

    /// Starts the group for a new E2E Space, with this device as its only member.
    /// # Errors
    /// Returns group creation or storage failures.
    pub fn create_group(&self, space: &str) -> Result<(), MlsError> {
        self.with(|p| {
            MlsGroup::new_with_group_id(
                p,
                &self.signer,
                &create_config(),
                group_id(space),
                self.credential(),
            )
            .map(drop)
            .map_err(mls)
        })
    }

    /// One commit that adds the devices behind `add` and removes every leaf of the
    /// identities in `remove`. Every package must carry a verified leaf. It applies (and
    /// the group moves on) when it comes back through [`Device::open`]; if the log refuses
    /// it, the next `open` drops it and this device can commit again.
    /// Commits adding `add` (key packages) and removing `remove`: identities (all their
    /// leaves) or single leaves named by [`leaf_name`].
    /// # Errors
    /// Refuses invalid added credentials/packages or failed commit staging, storage or encoding.
    pub fn commit(
        &self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<String>,
    ) -> Result<Commit, MlsError> {
        self.with(|p| {
            let kps = add
                .iter()
                .map(|b| {
                    let kp = validate_key_package(p.crypto(), b)?;
                    leaf_of(
                        kp.leaf_node().credential(),
                        kp.leaf_node().signature_key().as_slice(),
                    )?;
                    Ok(kp)
                })
                .collect::<Result<Vec<_>, MlsError>>()?;
            let mut group = Self::load(p, space)?;
            let gone = group
                .members()
                .filter(|m| {
                    leaf_of(&m.credential, &m.signature_key).is_ok_and(|l| {
                        remove.contains(&l.identity)
                            || remove.contains(&leaf_name(&l.identity, &l.device))
                    })
                })
                .map(|m| m.index)
                .collect::<Vec<_>>();
            let bundle = group
                .commit_builder()
                .propose_removals(gone)
                .propose_adds(kps)
                .load_psks(p.storage())
                .map_err(mls)?
                .build(p.rand(), p.crypto(), &self.signer, |_| true)
                .map_err(mls)?
                .stage_commit(p)
                .map_err(mls)?;
            commit_bytes(&bundle)
        })
    }

    /// Whether this device has a commit out that the log hasn't answered yet.
    #[must_use]
    pub fn pending(&self, space: &str) -> bool {
        self.recovery_pending(space)
            || self.with(|p| Self::load(p, space).is_ok_and(|g| g.pending_commit().is_some()))
    }

    /// Drops this device's commit after the relay refused it (another commit took the
    /// epoch). The winner arrives through the log; the device then commits again.
    /// # Errors
    /// Returns failures loading or clearing the pending/recovery commit.
    pub fn abandon(&self, space: &str) -> Result<(), MlsError> {
        if self.recovery_pending(space) {
            return self.abandon_recovery(space);
        }
        self.with(|p| {
            let mut group = Self::load(p, space)?;
            group.clear_pending_commit(p.storage()).map_err(mls)
        })
    }

    /// Joins from a Welcome found in `space`'s log. `Ok(false)` when it isn't for this
    /// device. The group's roster must equal `roster` (the log's members), or nothing is
    /// joined.
    /// # Errors
    /// Refuses malformed/wrong-space welcomes, unverified or unlisted leaves and storage failures.
    pub fn join(
        &self,
        space: &str,
        welcome: &[u8],
        roster: &BTreeSet<String>,
    ) -> Result<bool, MlsError> {
        self.with(|p| {
            let msg = MlsMessageIn::tls_deserialize_exact(welcome)
                .map_err(|e| MlsError::Decode(e.to_string()))?;
            let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
                return Err(MlsError::Decode("not a Welcome".into()));
            };
            let processed = match ProcessedWelcome::new_from_welcome(p, &join_config(), welcome) {
                Ok(w) => w,
                Err(WelcomeError::NoMatchingKeyPackage | WelcomeError::JoinerSecretNotFound) => {
                    return Ok(false)
                }
                Err(e) => return Err(welcome_error(e)),
            };
            let staged = processed
                .into_staged_welcome(p, None)
                .map_err(welcome_error)?;
            if staged.group_context().group_id() != &group_id(space) {
                return Err(MlsError::Mls("Welcome for another space".into()));
            }
            let group = staged
                .members()
                .map(|m| leaf_of(&m.credential, &m.signature_key).map(|l| l.identity))
                .collect::<Result<BTreeSet<_>, _>>()?;
            unlisted(group, roster)?;
            staged.into_group(p).map(drop).map_err(welcome_error)?;
            Ok(true)
        })
    }

    /// Encrypts an inner signed event for the group.
    /// # Errors
    /// Refuses pending recovery/reconciliation and missing groups; propagates encryption/storage failures.
    pub fn seal(&self, space: &str, plaintext: &[u8]) -> Result<Vec<u8>, MlsError> {
        if self.recovery_pending(space) || self.needs_reconciliation(space) {
            return Err(MlsError::Mls(
                "wait for the recovery commit to be sequenced".into(),
            ));
        }
        self.with(|p| {
            let mut group = Self::load(p, space)?;
            group
                .create_message(p, &self.signer, plaintext)
                .map_err(mls)?
                .to_bytes()
                .map_err(mls)
        })
    }
}
