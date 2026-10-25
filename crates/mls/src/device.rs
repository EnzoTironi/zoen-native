//! The device half: groups, sealing and opening, with state in the device's SQLite.

use std::collections::BTreeSet;

use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::RustCrypto;
use openmls_sqlite_storage::SqliteStorageProvider;
use rusqlite::Connection;

use crate::group::*;
use crate::sealed::{Keys, Scope, SealedCodec};
use crate::{leaf_name, leaf_of, mls, validate_key_package, Leaf, MlsError, SUITE};

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

    fn load(&self, p: &Provider<'c>, space: &str) -> Result<MlsGroup, MlsError> {
        MlsGroup::load(p.storage(), &group_id(space))
            .map_err(storage_error)?
            .ok_or(MlsError::NoGroup)
    }

    /// `count` single-use key packages, plus one last-resort package when asked: the one a
    /// claim falls back to when the others are gone. Their private keys stay sealed here.
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

    pub fn has_group(&self, space: &str) -> bool {
        self.with(|p| self.load(p, space).is_ok())
    }

    /// Reads the current group once. Storage failures never become missing groups.
    pub fn group_state(&self, space: &str) -> Result<GroupState, MlsError> {
        if self.recovery_pending(space) {
            return Ok(GroupState::Pending);
        }
        self.with(|p| match self.load(p, space) {
            Ok(group) if group.pending_commit().is_some() => Ok(GroupState::Pending),
            Ok(_) => Ok(GroupState::Ready),
            Err(MlsError::NoGroup) => Ok(GroupState::Missing),
            Err(error) => Err(error),
        })
    }

    /// Deletes this device's state for a group it was removed from: its secrets go, and
    /// a later Welcome (if it is added back) starts it afresh.
    pub fn forget(&self, space: &str) -> Result<(), MlsError> {
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction().map_err(mls))
            .transpose()?;
        self.with(|p| match self.load(p, space) {
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
            let mut group = self.load(p, space)?;
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
    pub fn pending(&self, space: &str) -> bool {
        self.recovery_pending(space)
            || self.with(|p| {
                self.load(p, space)
                    .is_ok_and(|g| g.pending_commit().is_some())
            })
    }

    /// Drops this device's commit after the relay refused it (another commit took the
    /// epoch). The winner arrives through the log; the device then commits again.
    pub fn abandon(&self, space: &str) -> Result<(), MlsError> {
        if self.recovery_pending(space) {
            return self.abandon_recovery(space);
        }
        self.with(|p| {
            let mut group = self.load(p, space)?;
            group.clear_pending_commit(p.storage()).map_err(mls)
        })
    }

    /// Joins from a Welcome found in `space`'s log. `Ok(false)` when it isn't for this
    /// device. The group's roster must equal `roster` (the log's members), or nothing is
    /// joined.
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
    pub fn seal(&self, space: &str, plaintext: &[u8]) -> Result<Vec<u8>, MlsError> {
        if self.recovery_pending(space) || self.needs_reconciliation(space) {
            return Err(MlsError::Mls(
                "wait for the recovery commit to be sequenced".into(),
            ));
        }
        self.with(|p| {
            let mut group = self.load(p, space)?;
            group
                .create_message(p, &self.signer, plaintext)
                .map_err(mls)?
                .to_bytes()
                .map_err(mls)
        })
    }

    /// Processes one sealed Application or Commit from the log, in log order. `roster` is
    /// the log's member identities after this point; a commit leaving any other roster is
    /// refused and not applied.
    pub fn open(
        &self,
        space: &str,
        data: &[u8],
        roster: &BTreeSet<String>,
        admitted_sender: &Leaf,
    ) -> Result<Opened, MlsError> {
        self.with(|p| {
            let mut group = match self.load(p, space) {
                Ok(group) => group,
                Err(MlsError::NoGroup) => return Ok(Opened::NotMember),
                Err(error) => return Err(error),
            };
            if !group.is_active() {
                return Ok(Opened::NotMember);
            }
            let msg = MlsMessageIn::tls_deserialize_exact(data)
                .map_err(|e| MlsError::Decode(e.to_string()))?;
            let msg = msg
                .try_into_protocol_message()
                .map_err(|e| MlsError::Decode(format!("{e:?}")))?;
            let (kind, epoch) = (msg.content_type(), msg.epoch());
            let processed = match group.process_message(p, msg) {
                Ok(m) => m,
                Err(ProcessMessageError::ValidationError(ValidationError::WrongEpoch)) => {
                    return Ok(Opened::Stale)
                }
                Err(ProcessMessageError::StorageError(error)) => return Err(storage_error(error)),
                // OpenMLS also collapses failed local key reads into this staging error.
                Err(
                    error @ ProcessMessageError::InvalidCommit(
                        StageCommitError::MissingDecryptionKey,
                    ),
                ) => {
                    return Err(storage_error(error));
                }
                Err(e) => return Err(mls(e)),
            };
            let sender = processed.sender().clone();
            match processed.into_content() {
                ProcessedMessageContent::ApplicationMessage(m) => {
                    let Sender::Member(index) = sender else {
                        return Err(MlsError::Mls(
                            "application message from a non-member".into(),
                        ));
                    };
                    let member = group
                        .members()
                        .find(|m| m.index == index)
                        .ok_or(MlsError::Credential)?;
                    Ok(Opened::Application {
                        plaintext: m.into_bytes(),
                        from: leaf_of(&member.credential, &member.signature_key)?,
                    })
                }
                ProcessedMessageContent::StagedCommitMessage(staged) => {
                    if sender == Sender::NewMemberCommit {
                        let path = staged.update_path_leaf_node().ok_or(MlsError::Credential)?;
                        let leaf = leaf_of(path.credential(), path.signature_key().as_slice())?;
                        unlisted(BTreeSet::from([leaf.identity.clone()]), roster)?;
                        if leaf.identity != admitted_sender.identity
                            || leaf.device != admitted_sender.device
                        {
                            return Err(MlsError::Credential);
                        }
                    }
                    let reconcile = match check_roster(&group, &staged, roster) {
                        Ok(()) => false,
                        Err(MlsError::Unlisted(extra))
                            if sender == Sender::NewMemberCommit
                                && extra.is_subset(&identities(&group)?) =>
                        {
                            true
                        }
                        Err(err) => return Err(err),
                    };
                    let tx = self
                        .conn
                        .is_autocommit()
                        .then(|| self.conn.unchecked_transaction())
                        .transpose()
                        .map_err(|e| MlsError::Storage(e.to_string()))?;
                    group
                        .merge_staged_commit(p, *staged)
                        .map_err(|error| match error {
                            MergeCommitError::StorageError(error) => storage_error(error),
                            error => mls(error),
                        })?;
                    self.set_reconciliation(space, reconcile)?;
                    let epoch = group.epoch().as_u64();
                    if !group.is_active() {
                        // Our leaf was taken out (a removal, or a stranded leaf replaced):
                        // the secrets go, and a later Welcome starts the group afresh.
                        group.delete(p.storage()).map_err(storage_error)?;
                    }
                    if let Some(tx) = tx {
                        tx.commit().map_err(|e| MlsError::Storage(e.to_string()))?;
                    }
                    Ok(if reconcile {
                        Opened::Reconcile { epoch }
                    } else {
                        Opened::Commit { epoch }
                    })
                }
                ProcessedMessageContent::OwnPendingCommit => self.merge_own(p, &mut group, roster),
                // Our own PrivateMessage: undecryptable by its author. A commit of ours at the
                // current epoch is the pending one (OpenMLS allows one at a time), so it won;
                // any other is a commit that lost the race.
                ProcessedMessageContent::OwnPrivateMessage if kind == ContentType::Commit => {
                    if epoch == group.epoch() && group.pending_commit().is_some() {
                        self.merge_own(p, &mut group, roster)
                    } else {
                        Ok(Opened::Stale)
                    }
                }
                ProcessedMessageContent::OwnPrivateMessage => Ok(Opened::Own),
                ProcessedMessageContent::ProposalMessage(_)
                | ProcessedMessageContent::ExternalJoinProposalMessage(_) => Err(MlsError::Mls(
                    "standalone proposals aren't used; commits carry them".into(),
                )),
            }
        })
    }

    fn merge_own(
        &self,
        p: &Provider<'c>,
        group: &mut MlsGroup,
        roster: &BTreeSet<String>,
    ) -> Result<Opened, MlsError> {
        let staged = group.pending_commit().ok_or(MlsError::NoGroup)?;
        if let Err(e) = check_roster(group, staged, roster) {
            // Everyone refuses it alike, so drop it and stay in this epoch.
            group
                .clear_pending_commit(p.storage())
                .map_err(storage_error)?;
            return Err(e);
        }
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()
            .map_err(|e| MlsError::Storage(e.to_string()))?;
        group.merge_pending_commit(p).map_err(|error| match error {
            MergePendingCommitError::MergeCommitError(MergeCommitError::StorageError(error)) => {
                storage_error(error)
            }
            error => mls(error),
        })?;
        self.set_reconciliation(
            std::str::from_utf8(group.group_id().as_slice()).map_err(|_| MlsError::Credential)?,
            false,
        )?;
        if let Some(tx) = tx {
            tx.commit().map_err(|e| MlsError::Storage(e.to_string()))?;
        }
        Ok(Opened::Commit {
            epoch: group.epoch().as_u64(),
        })
    }
}
