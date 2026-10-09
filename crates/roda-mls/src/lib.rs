//! # roda-mls: end-to-end Spaces on OpenMLS (ADR 0026)
//!
//! One [`Device`] is one MLS leaf. Its signature key is the device's Ed25519 key, and its
//! credential is a [`Leaf`]: identity, device and the identity's certificate over it.
//! The group id is the Space id. The relay orders every MLS message in the Space's log,
//! so log order is epoch order; this crate turns those bytes into plaintext and back.
//!
//! - Membership is member-checked: [`Device::open`] applies a commit only if the roster it
//!   leaves equals the roster the log says, so a relay can't slip a reader into a group.
//! - A sender can't decrypt its own application messages ([`Opened::Own`]). It keeps what it
//!   sent, as every client already does.
//! - State lives in the device's SQLite database, every value sealed with a state key from
//!   the SecretVault ([`sealed`]).
//! - [`Device::checkpoint`] gives the epoch and the digest members sign into `Checkpoint`
//!   events, so members can tell when they were shown different groups.

mod leaf;
pub mod sealed;

use std::collections::BTreeSet;

pub use leaf::Leaf;
use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::RustCrypto;
use openmls_sqlite_storage::SqliteStorageProvider;
use rusqlite::Connection;
use sealed::{Scope, SealedCodec};
use sha2::{Digest, Sha256};

/// `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`, the value in `Sealed.suite`.
pub const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;
/// Past epochs whose keys stay around for messages sequenced just after a commit.
const PAST_EPOCHS: usize = 4;
/// Application messages pad to a multiple of this, hiding exact lengths from the relay.
const PADDING: usize = 64;
const CHECKPOINT_TAG: &[u8] = b"zoen-checkpoint/1";

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("MLS: {0}")]
    Mls(String),
    #[error("not valid MLS bytes: {0}")]
    Decode(String),
    #[error("a leaf whose credential doesn't verify")]
    Credential,
    #[error("no group for this space on this device")]
    NoGroup,
    #[error("the commit leaves {got:?} in the group, the log says {expected:?}")]
    RosterMismatch {
        expected: BTreeSet<String>,
        got: BTreeSet<String>,
    },
}

fn mls(e: impl std::fmt::Debug) -> MlsError {
    MlsError::Mls(format!("{e:?}"))
}

/// A commit this device made: publish `commit`, then `welcome`, in the Space's log.
/// The commit applies when it comes back through [`Device::open`].
pub struct Added {
    pub commit: Vec<u8>,
    pub welcome: Vec<u8>,
}

/// What a sealed envelope turned out to be.
#[derive(Debug, PartialEq)]
pub enum Opened {
    /// A member's message: the inner signed event's bytes, and who sent it.
    Application { plaintext: Vec<u8>, from: Leaf },
    /// A commit, applied: the group is now at `epoch`.
    Commit { epoch: u64 },
    /// This device's own application message coming back. It keeps what it sent.
    Own,
    /// A commit for an epoch already gone (another commit won the race), ignored by
    /// every member alike.
    Stale,
    /// Not for this device: before it joined, or after it left.
    NotMember,
}

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

fn create_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(PAST_EPOCHS)
        .padding_size(PADDING)
        .build()
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(PAST_EPOCHS)
        .padding_size(PADDING)
        .build()
}

fn group_id(space: &str) -> GroupId {
    GroupId::from_slice(space.as_bytes())
}

/// The verified leaf of a member, or `Credential` when it doesn't verify.
fn leaf_of(credential: &Credential, signature_key: &[u8]) -> Result<Leaf, MlsError> {
    let basic = BasicCredential::try_from(credential.clone()).map_err(|_| MlsError::Credential)?;
    Leaf::verified(basic.identity(), signature_key).ok_or(MlsError::Credential)
}

fn identities(group: &MlsGroup) -> Result<BTreeSet<String>, MlsError> {
    group
        .members()
        .map(|m| leaf_of(&m.credential, &m.signature_key).map(|l| l.identity))
        .collect()
}

/// The roster `staged` leaves: current members minus removals plus additions. It must be
/// the log's roster, or the commit is refused before it touches the group.
fn check_roster(
    group: &MlsGroup,
    staged: &StagedCommit,
    roster: &BTreeSet<String>,
) -> Result<(), MlsError> {
    let removed: Vec<LeafNodeIndex> = staged
        .remove_proposals()
        .map(|r| r.remove_proposal().removed())
        .collect();
    let mut got = BTreeSet::new();
    for m in group.members().filter(|m| !removed.contains(&m.index)) {
        got.insert(leaf_of(&m.credential, &m.signature_key)?.identity);
    }
    for add in staged.add_proposals() {
        let leaf = add.add_proposal().key_package().leaf_node();
        got.insert(leaf_of(leaf.credential(), leaf.signature_key().as_slice())?.identity);
    }
    if &got == roster {
        Ok(())
    } else {
        Err(MlsError::RosterMismatch {
            expected: roster.clone(),
            got,
        })
    }
}

/// Checks a key package someone published and returns whose device it is. The relay
/// uses this to refuse packages that don't name the device publishing them.
pub fn key_package_leaf(bytes: &[u8]) -> Result<Leaf, MlsError> {
    let kp = validate_key_package(&RustCrypto::default(), bytes)?;
    leaf_of(
        kp.leaf_node().credential(),
        kp.leaf_node().signature_key().as_slice(),
    )
}

fn validate_key_package(crypto: &RustCrypto, bytes: &[u8]) -> Result<KeyPackage, MlsError> {
    let kp =
        KeyPackageIn::tls_deserialize_exact(bytes).map_err(|e| MlsError::Decode(e.to_string()))?;
    let kp = kp.validate(crypto, ProtocolVersion::Mls10).map_err(mls)?;
    if kp.ciphersuite() != SUITE {
        return Err(MlsError::Mls("key package for another ciphersuite".into()));
    }
    Ok(kp)
}

/// Creates the MLS tables in the device database. Idempotent; run at every open.
pub fn migrate(conn: &mut Connection) -> Result<(), MlsError> {
    SqliteStorageProvider::<SealedCodec, &mut Connection>::new(conn)
        .run_migrations()
        .map_err(mls)
}

/// This device as an MLS client, over the device database.
pub struct Device<'c> {
    conn: &'c Connection,
    state_key: [u8; 32],
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
        let device = roda_log::Signer::from_secret(&device_secret).id();
        let public = hex::decode(&device).map_err(mls)?;
        let leaf = Leaf {
            identity: identity.to_string(),
            device,
            cert: cert.to_string(),
        };
        if Leaf::verified(&leaf.encode(), &public).is_none() {
            return Err(MlsError::Credential);
        }
        Ok(Self {
            conn,
            state_key,
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
        let _scope = Scope::enter(self.state_key);
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
            .map_err(mls)?
            .ok_or(MlsError::NoGroup)
    }

    /// `count` single-use key packages, plus one last-resort package when asked: the one a
    /// claim falls back to when the others are gone. Their private keys stay sealed here.
    pub fn key_packages(&self, count: usize, last_resort: bool) -> Result<Vec<Vec<u8>>, MlsError> {
        self.with(|p| {
            (0..count + usize::from(last_resort))
                .map(|i| {
                    let mut b = KeyPackage::builder();
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

    /// Adds the devices behind `key_packages`. Every package must carry a verified leaf.
    pub fn add(&self, space: &str, key_packages: &[Vec<u8>]) -> Result<Added, MlsError> {
        self.with(|p| {
            let kps = key_packages
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
            let (commit, welcome, _) = group.add_members(p, &self.signer, &kps).map_err(mls)?;
            Ok(Added {
                commit: commit.to_bytes().map_err(mls)?,
                welcome: welcome.to_bytes().map_err(mls)?,
            })
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
                Err(e) => return Err(mls(e)),
            };
            let staged = processed.into_staged_welcome(p, None).map_err(mls)?;
            if staged.group_context().group_id() != &group_id(space) {
                return Err(MlsError::Mls("Welcome for another space".into()));
            }
            let got = staged
                .members()
                .map(|m| leaf_of(&m.credential, &m.signature_key).map(|l| l.identity))
                .collect::<Result<BTreeSet<_>, _>>()?;
            if &got != roster {
                return Err(MlsError::RosterMismatch {
                    expected: roster.clone(),
                    got,
                });
            }
            staged.into_group(p).map(drop).map_err(mls)?;
            Ok(true)
        })
    }

    /// Encrypts an inner signed event for the group.
    pub fn seal(&self, space: &str, plaintext: &[u8]) -> Result<Vec<u8>, MlsError> {
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
    ) -> Result<Opened, MlsError> {
        self.with(|p| {
            let Ok(mut group) = self.load(p, space) else {
                return Ok(Opened::NotMember);
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
                    check_roster(&group, &staged, roster)?;
                    group.merge_staged_commit(p, *staged).map_err(mls)?;
                    Ok(Opened::Commit {
                        epoch: group.epoch().as_u64(),
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
        check_roster(group, staged, roster)?;
        group.merge_pending_commit(p).map_err(mls)?;
        Ok(Opened::Commit {
            epoch: group.epoch().as_u64(),
        })
    }

    /// Member identities in the group as this device has it.
    pub fn roster(&self, space: &str) -> Result<BTreeSet<String>, MlsError> {
        self.with(|p| identities(&self.load(p, space)?))
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

#[cfg(test)]
mod tests;
