//! `OpenMLS` groups over a caller-owned provider and signer. Private device keys stay
//! in the host. The host must commit the complete provider snapshot before publishing.
//! A snapshot opens only with the signer that owns its own leaf.
//! A blank leaf, or a public group with no own index, is not membership.
//! forget deletes any stored group for the space.
mod views;

use std::collections::BTreeSet;

use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::signatures::Signer;
use openmls_traits::storage::StorageProvider;

use crate::group::{
    capabilities, check_roster, commit_bytes, create_config, group_id, join_config, storage_error,
    unlisted, welcome_error, Commit, GroupState, Opened,
};
use crate::{leaf_of, mls, validate_key_package, Leaf, MlsError, SUITE};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnLeaf {
    Ours,
    Removed,
    Foreign,
}

enum Snapshot {
    Absent,
    Stale(Option<MlsGroup>),
    Ours(MlsGroup),
    Foreign(MlsGroup),
}

fn membership_wait() -> MlsError {
    MlsError::Mls("wait for the membership commit".into())
}

pub struct ExternalDevice<'a, P, S> {
    provider: &'a P,
    signer: &'a S,
    leaf: Leaf,
    public: Vec<u8>,
}

impl<'a, P, S> ExternalDevice<'a, P, S>
where
    P: OpenMlsProvider<CryptoProvider = RustCrypto, RandProvider = RustCrypto>,
    S: Signer,
{
    /// # Errors
    /// Refuses unsupported signing algorithms, invalid certified leaf keys and signatures not produced by that leaf.
    pub fn new(
        provider: &'a P,
        signer: &'a S,
        leaf: Leaf,
        signature_public: Vec<u8>,
    ) -> Result<Self, MlsError> {
        if signer.signature_scheme() != SUITE.signature_algorithm()
            || Leaf::verified(&leaf.encode(), &signature_public).as_ref() != Some(&leaf)
        {
            return Err(MlsError::Credential);
        }
        let proof = signer.sign(&leaf.encode()).map_err(mls)?;
        if !event_log::verify_sig(&leaf.device, &leaf.encode(), &hex::encode(proof)) {
            return Err(MlsError::Credential);
        }
        Ok(Self {
            provider,
            signer,
            leaf,
            public: signature_public,
        })
    }

    #[must_use]
    pub fn leaf(&self) -> &Leaf {
        &self.leaf
    }

    fn credential(&self) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(self.leaf.encode()).into(),
            signature_key: self.public.clone().into(),
        }
    }

    fn snapshot(&self, space: &str) -> Result<Snapshot, MlsError> {
        let id = group_id(space);
        let storage = self.provider.storage();
        let own_index: Option<LeafNodeIndex> =
            storage.own_leaf_index(&id).map_err(storage_error)?;
        if own_index.is_none() {
            let public = PublicGroup::load(storage, &id).map_err(storage_error)?;
            return Ok(if public.is_some() {
                Snapshot::Stale(None)
            } else {
                Snapshot::Absent
            });
        }
        let Some(group) = MlsGroup::load(storage, &id).map_err(storage_error)? else {
            return Ok(Snapshot::Stale(None));
        };
        if group.ciphersuite() != SUITE {
            return Ok(Snapshot::Foreign(group));
        }
        Ok(match self.own_leaf(&group) {
            OwnLeaf::Ours => Snapshot::Ours(group),
            OwnLeaf::Removed => Snapshot::Stale(Some(group)),
            OwnLeaf::Foreign => Snapshot::Foreign(group),
        })
    }

    fn own_leaf(&self, group: &MlsGroup) -> OwnLeaf {
        let leaves = group.treesync().leaves();
        let Some(slot) = leaves.get(group.own_leaf_index().usize()) else {
            return OwnLeaf::Foreign;
        };
        let Some(node) = slot.as_ref() else {
            return OwnLeaf::Removed;
        };
        if node.signature_key().as_slice() != self.public.as_slice() {
            return OwnLeaf::Foreign;
        }
        match leaf_of(node.credential(), node.signature_key().as_slice()) {
            Ok(leaf) if leaf == self.leaf => OwnLeaf::Ours,
            _ => OwnLeaf::Foreign,
        }
    }

    fn load(&self, space: &str) -> Result<MlsGroup, MlsError> {
        match self.snapshot(space)? {
            Snapshot::Ours(group) | Snapshot::Stale(Some(group)) => Ok(group),
            Snapshot::Absent | Snapshot::Stale(None) => Err(MlsError::NoGroup),
            Snapshot::Foreign(_) => Err(MlsError::Credential),
        }
    }

    fn drop_stored(&self, space: &str, group: Option<MlsGroup>) -> Result<(), MlsError> {
        if let Some(mut group) = group {
            return group.delete(self.provider.storage()).map_err(storage_error);
        }
        let id = group_id(space);
        let storage = self.provider.storage();
        if let Some(public) = PublicGroup::load(storage, &id).map_err(storage_error)? {
            let epoch = public.group_context().epoch();
            let count = u32::try_from(public.treesync().leaves().len()).unwrap_or(0);
            for index in 0..count {
                storage
                    .delete_encryption_epoch_key_pairs(&id, &epoch, index)
                    .map_err(storage_error)?;
            }
        }
        PublicGroup::delete(storage, &id).map_err(storage_error)?;
        storage.delete_own_leaf_index(&id).map_err(storage_error)?;
        storage
            .delete_group_epoch_secrets(&id)
            .map_err(storage_error)?;
        storage.delete_message_secrets(&id).map_err(storage_error)?;
        storage
            .delete_all_resumption_psk_secrets(&id)
            .map_err(storage_error)?;
        storage.delete_group_config(&id).map_err(storage_error)?;
        storage.delete_own_leaf_nodes(&id).map_err(storage_error)?;
        storage.delete_group_state(&id).map_err(storage_error)?;
        storage
            .clear_proposal_queue::<GroupId, openmls::ciphersuite::hash_ref::ProposalRef>(&id)
            .map_err(storage_error)?;
        Ok(())
    }

    /// # Errors
    /// Propagates foreign-credential and storage failures while absent/stale snapshots remain missing groups.
    pub fn group_state(&self, space: &str) -> Result<GroupState, MlsError> {
        match self.snapshot(space)? {
            Snapshot::Absent | Snapshot::Stale(_) => Ok(GroupState::Missing),
            Snapshot::Foreign(_) => Err(MlsError::Credential),
            Snapshot::Ours(group) => {
                if group.pending_commit().is_some() {
                    Ok(GroupState::Pending)
                } else if group.is_active() {
                    Ok(GroupState::Ready)
                } else {
                    Ok(GroupState::Missing)
                }
            }
        }
    }

    /// # Errors
    /// Refuses more than 64 requested packages, invalid generated credentials and generation/storage/encoding failures.
    pub fn key_packages(&self, count: usize, last_resort: bool) -> Result<Vec<Vec<u8>>, MlsError> {
        if count > 64 {
            return Err(MlsError::Mls("too many browser key packages".into()));
        }
        (0..count + usize::from(last_resort))
            .map(|index| {
                let mut builder = KeyPackage::builder().leaf_node_capabilities(capabilities());
                if last_resort && index == count {
                    builder = builder.mark_as_last_resort();
                }
                let bundle = builder
                    .build(SUITE, self.provider, self.signer, self.credential())
                    .map_err(mls)?;
                let bytes = bundle.key_package().tls_serialize_detached().map_err(mls)?;
                if crate::key_package_leaf(&bytes)? != self.leaf {
                    return Err(MlsError::Credential);
                }
                Ok(bytes)
            })
            .collect()
    }

    /// # Errors
    /// Refuses existing, stale or foreign snapshots and propagates group creation/storage failures.
    pub fn create_group(&self, space: &str) -> Result<(), MlsError> {
        match self.snapshot(space)? {
            Snapshot::Absent => {}
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Stale(_) | Snapshot::Ours(_) => {
                return Err(MlsError::Mls("group already exists".into()));
            }
        }
        MlsGroup::new_with_group_id(
            self.provider,
            self.signer,
            &create_config(),
            group_id(space),
            self.credential(),
        )
        .map(drop)
        .map_err(mls)
    }

    /// # Errors
    /// Refuses foreign snapshots, malformed or wrong-group/suite welcomes, unverified/unlisted leaves and storage failures. No matching package returns false.
    pub fn join(
        &self,
        space: &str,
        welcome: &[u8],
        roster: &BTreeSet<String>,
    ) -> Result<bool, MlsError> {
        let stale = match self.snapshot(space)? {
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Stale(stored) => Some(stored),
            Snapshot::Absent | Snapshot::Ours(_) => None,
        };
        let msg = MlsMessageIn::tls_deserialize_exact(welcome)
            .map_err(|e| MlsError::Decode(e.to_string()))?;
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            return Err(MlsError::Decode("not a Welcome".into()));
        };
        let processed =
            match ProcessedWelcome::new_from_welcome(self.provider, &join_config(), welcome) {
                Ok(processed) => processed,
                Err(WelcomeError::NoMatchingKeyPackage | WelcomeError::JoinerSecretNotFound) => {
                    return Ok(false)
                }
                Err(error) => return Err(welcome_error(error)),
            };
        if let Some(stored) = stale {
            self.drop_stored(space, stored)?;
        }
        let staged = processed
            .into_staged_welcome(self.provider, None)
            .map_err(welcome_error)?;
        if staged.group_context().group_id() != &group_id(space)
            || staged.group_context().ciphersuite() != SUITE
        {
            return Err(MlsError::Mls(
                "Welcome for another group or ciphersuite".into(),
            ));
        }
        let members = staged
            .members()
            .map(|member| {
                leaf_of(&member.credential, &member.signature_key).map(|leaf| leaf.identity)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        unlisted(members, roster)?;
        staged
            .into_group(self.provider)
            .map(drop)
            .map_err(welcome_error)?;
        Ok(true)
    }

    /// # Errors
    /// Refuses absent/stale/foreign membership, inactive groups or pending commits and propagates encryption/storage failures.
    pub fn seal(&self, space: &str, plaintext: &[u8]) -> Result<Vec<u8>, MlsError> {
        let mut group = match self.snapshot(space)? {
            Snapshot::Absent => return Err(MlsError::NoGroup),
            Snapshot::Stale(_) => return Err(membership_wait()),
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Ours(group) => group,
        };
        if !group.is_active() || group.pending_commit().is_some() {
            return Err(membership_wait());
        }
        group
            .create_message(self.provider, self.signer, plaintext)
            .map_err(mls)?
            .to_bytes()
            .map_err(mls)
    }

    /// # Errors
    /// Refuses invalid packages/credentials, absent/stale/foreign groups and commit staging/storage/encoding failures.
    pub fn commit(
        &self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<String>,
    ) -> Result<Commit, MlsError> {
        let packages = add
            .iter()
            .map(|bytes| {
                let package = validate_key_package(self.provider.crypto(), bytes)?;
                leaf_of(
                    package.leaf_node().credential(),
                    package.leaf_node().signature_key().as_slice(),
                )?;
                Ok(package)
            })
            .collect::<Result<Vec<_>, MlsError>>()?;
        let mut group = match self.snapshot(space)? {
            Snapshot::Absent => return Err(MlsError::NoGroup),
            Snapshot::Stale(_) => return Err(membership_wait()),
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Ours(group) => group,
        };
        if !group.is_active() {
            return Err(membership_wait());
        }
        let gone = group
            .members()
            .filter_map(|member| {
                let leaf = leaf_of(&member.credential, &member.signature_key).ok()?;
                (remove.contains(&leaf.identity)
                    || remove.contains(&crate::leaf_name(&leaf.identity, &leaf.device)))
                .then_some(member.index)
            })
            .collect::<Vec<_>>();
        let bundle = group
            .commit_builder()
            .propose_adds(packages)
            .propose_removals(gone)
            .load_psks(self.provider.storage())
            .map_err(mls)?
            .build(
                self.provider.rand(),
                self.provider.crypto(),
                self.signer,
                |_| true,
            )
            .map_err(mls)?
            .stage_commit(self.provider)
            .map_err(mls)?;
        commit_bytes(&bundle)
    }

    /// # Errors
    /// Refuses invalid framing, authentication, admitted-sender/roster mismatch or storage failures. Missing membership and stale epochs remain explicit outcomes.
    pub fn open(
        &self,
        space: &str,
        data: &[u8],
        roster: &BTreeSet<String>,
        admitted_sender: &Leaf,
    ) -> Result<Opened, MlsError> {
        let mut group = match self.snapshot(space)? {
            Snapshot::Absent | Snapshot::Stale(_) => return Ok(Opened::NotMember),
            Snapshot::Foreign(_) => return Err(MlsError::Credential),
            Snapshot::Ours(group) => group,
        };
        if !group.is_active() {
            return Ok(Opened::NotMember);
        }
        let msg = MlsMessageIn::tls_deserialize_exact(data)
            .map_err(|e| MlsError::Decode(e.to_string()))?
            .try_into_protocol_message()
            .map_err(|e| MlsError::Decode(format!("{e:?}")))?;
        let (kind, epoch) = (msg.content_type(), msg.epoch());
        let processed = match group.process_message(self.provider, msg) {
            Ok(processed) => processed,
            Err(ProcessMessageError::ValidationError(ValidationError::WrongEpoch)) => {
                return Ok(Opened::Stale)
            }
            Err(ProcessMessageError::StorageError(error)) => return Err(storage_error(error)),
            Err(
                error @ ProcessMessageError::InvalidCommit(StageCommitError::MissingDecryptionKey),
            ) => return Err(storage_error(error)),
            Err(error) => return Err(mls(error)),
        };
        let sender = processed.sender().clone();
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => {
                let Sender::Member(index) = sender else {
                    return Err(MlsError::Credential);
                };
                let member = group
                    .members()
                    .find(|member| member.index == index)
                    .ok_or(MlsError::Credential)?;
                Ok(Opened::Application {
                    plaintext: message.into_bytes(),
                    from: leaf_of(&member.credential, &member.signature_key)?,
                })
            }
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                if sender == Sender::NewMemberCommit {
                    let path = staged.update_path_leaf_node().ok_or(MlsError::Credential)?;
                    let leaf = leaf_of(path.credential(), path.signature_key().as_slice())?;
                    if leaf.identity != admitted_sender.identity
                        || leaf.device != admitted_sender.device
                    {
                        return Err(MlsError::Credential);
                    }
                }
                // A host without recovery reconciliation must refuse the commit rather
                // than seal applications to a leaf absent from its confirmed roster.
                check_roster(&group, &staged, roster)?;
                group
                    .merge_staged_commit(self.provider, *staged)
                    .map_err(|error| match error {
                        MergeCommitError::StorageError(error) => storage_error(error),
                        error @ MergeCommitError::LibraryError(_) => mls(error),
                    })?;
                let epoch = group.epoch().as_u64();
                if !group.is_active() {
                    group
                        .delete(self.provider.storage())
                        .map_err(storage_error)?;
                }
                Ok(Opened::Commit { epoch })
            }
            ProcessedMessageContent::OwnPendingCommit => self.merge_own(&mut group, roster),
            ProcessedMessageContent::OwnPrivateMessage if kind == ContentType::Commit => {
                if epoch == group.epoch() && group.pending_commit().is_some() {
                    self.merge_own(&mut group, roster)
                } else {
                    Ok(Opened::Stale)
                }
            }
            ProcessedMessageContent::OwnPrivateMessage => Ok(Opened::Own),
            ProcessedMessageContent::ProposalMessage(_)
            | ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                Err(MlsError::Mls("standalone proposals aren't used".into()))
            }
        }
    }

    fn merge_own(
        &self,
        group: &mut MlsGroup,
        roster: &BTreeSet<String>,
    ) -> Result<Opened, MlsError> {
        check_roster(
            group,
            group.pending_commit().ok_or(MlsError::NoGroup)?,
            roster,
        )?;
        group.merge_pending_commit(self.provider).map_err(mls)?;
        Ok(Opened::Commit {
            epoch: group.epoch().as_u64(),
        })
    }
}

#[cfg(all(test, feature = "device"))]
mod tests;
