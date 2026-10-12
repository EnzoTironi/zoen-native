//! Group configuration and roster checks shared by SQLite and browser adapters.
use crate::{leaf_of, mls, Leaf, MlsError, SUITE};
use openmls::prelude::{tls_codec::Serialize as _, *};
use std::collections::BTreeSet;

/// Past epochs whose keys stay around for messages sequenced just after a commit.
const PAST_EPOCHS: usize = 4;
/// Application messages pad to a multiple of this, hiding exact lengths from the relay.
const PADDING: usize = 64;
pub(crate) const CHECKPOINT_TAG: &[u8] = b"zoen-checkpoint/1";

pub(crate) fn storage_error(error: impl std::fmt::Debug) -> MlsError {
    MlsError::Storage(format!("{error:?}"))
}

pub(crate) fn welcome_error<E: std::fmt::Debug>(error: WelcomeError<E>) -> MlsError {
    match error {
        WelcomeError::StorageError(error)
        | WelcomeError::PublicGroupError(CreationFromExternalError::WriteToStorageError(error)) => {
            storage_error(error)
        }
        error => mls(error),
    }
}

/// A commit this device made: publish `commit`, then `welcome` (when it adds someone),
/// in the Space's log. The commit applies when it comes back through the device's open.
pub struct Commit {
    pub commit: Vec<u8>,
    pub welcome: Option<Vec<u8>>,
    /// Public, signed context for the resulting epoch; never an epoch secret.
    pub group_info: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupState {
    Missing,
    Pending,
    Ready,
}

pub(crate) fn commit_bytes(bundle: &CommitMessageBundle) -> Result<Commit, MlsError> {
    Ok(Commit {
        commit: bundle.commit().to_bytes().map_err(mls)?,
        welcome: bundle
            .to_welcome_msg()
            .map(|w| w.to_bytes())
            .transpose()
            .map_err(mls)?,
        group_info: bundle
            .group_info()
            .ok_or_else(|| MlsError::Mls("commit has no GroupInfo".into()))?
            .tls_serialize_detached()
            .map_err(mls)?,
    })
}

/// What a sealed envelope turned out to be.
#[derive(Debug, PartialEq)]
pub enum Opened {
    /// A member's message: the inner signed event's bytes, and who sent it.
    Application { plaintext: Vec<u8>, from: Leaf },
    /// A commit, applied: the group is now at `epoch`.
    Commit { epoch: u64 },
    /// A confirmed external join retained members removed while it was in
    /// flight. Applications stay held until a sequenced removal commit.
    Reconcile { epoch: u64 },
    /// This device's own application message coming back. It keeps what it sent.
    Own,
    /// A commit for an epoch already gone (another commit won the race), ignored by
    /// every member alike.
    Stale,
    /// Not for this device: before it joined, or after it left.
    NotMember,
}

/// Current encryption keys indexed by the verified identity and device in each leaf.
pub type LeafKeys = std::collections::BTreeMap<(String, String), Vec<u8>>;

/// Every leaf advertises the last-resort extension, so a device's last-resort package
/// passes validation and its leaf looks the same as the ones from single-use packages.
pub(crate) fn capabilities() -> Capabilities {
    Capabilities::new(None, None, Some(&[ExtensionType::LastResort]), None, None)
}

pub(crate) fn create_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .capabilities(capabilities())
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(PAST_EPOCHS)
        .padding_size(PADDING)
        .build()
}

pub(crate) fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(PAST_EPOCHS)
        .padding_size(PADDING)
        .build()
}

pub(crate) fn group_id(space: &str) -> GroupId {
    GroupId::from_slice(space.as_bytes())
}

pub(crate) fn identities(group: &MlsGroup) -> Result<BTreeSet<String>, MlsError> {
    group
        .members()
        .map(|m| leaf_of(&m.credential, &m.signature_key).map(|l| l.identity))
        .collect()
}

pub(crate) fn leaf_keys(group: &MlsGroup) -> Result<LeafKeys, MlsError> {
    group
        .members()
        .map(|m| {
            leaf_of(&m.credential, &m.signature_key)
                .map(|l| ((l.identity, l.device), m.encryption_key.clone()))
        })
        .collect()
}

/// Members the group holds after `staged` that the log doesn't list. A commit is applied
/// only when this is empty: the group never holds a reader the log (and so everyone) can't
/// see. Listed people the group lacks yet are fine; an admin's next commit adds them.
pub(crate) fn check_roster(
    group: &MlsGroup,
    staged: &StagedCommit,
    roster: &BTreeSet<String>,
) -> Result<(), MlsError> {
    let removed: Vec<LeafNodeIndex> = staged
        .remove_proposals()
        .map(|r| r.remove_proposal().removed())
        .collect();
    let mut after = BTreeSet::new();
    for m in group.members().filter(|m| !removed.contains(&m.index)) {
        after.insert(leaf_of(&m.credential, &m.signature_key)?.identity);
    }
    for add in staged.add_proposals() {
        let leaf = add.add_proposal().key_package().leaf_node();
        let id = leaf_of(leaf.credential(), leaf.signature_key().as_slice())?.identity;
        unlisted(BTreeSet::from([id.clone()]), roster)?;
        after.insert(id);
    }
    if let Some(path) = staged.update_path_leaf_node() {
        let id = leaf_of(path.credential(), path.signature_key().as_slice())?.identity;
        unlisted(BTreeSet::from([id.clone()]), roster)?;
        after.insert(id);
    }
    unlisted(after, roster)
}

pub(crate) fn unlisted(group: BTreeSet<String>, roster: &BTreeSet<String>) -> Result<(), MlsError> {
    let extra: BTreeSet<String> = group.difference(roster).cloned().collect();
    if extra.is_empty() {
        Ok(())
    } else {
        Err(MlsError::Unlisted(extra))
    }
}
