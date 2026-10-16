//! OpenMLS groups over a caller-owned provider and signer. Private device keys stay
//! in the host. The host must commit the complete provider snapshot before publishing.
//! A snapshot opens only with the signer that owns its own leaf.
//! A blank leaf, or a public group with no own index, is not membership.
//! forget deletes any stored group for the space.
use std::collections::BTreeSet;

use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::signatures::Signer;
use openmls_traits::storage::StorageProvider;
use sha2::{Digest, Sha256};

use crate::group::*;
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
        if !zoen_log::verify_sig(&leaf.device, &leaf.encode(), &hex::encode(proof)) {
            return Err(MlsError::Credential);
        }
        Ok(Self {
            provider,
            signer,
            leaf,
            public: signature_public,
        })
    }

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
                        error => mls(error),
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

    pub fn forget(&self, space: &str) -> Result<(), MlsError> {
        match self.snapshot(space)? {
            Snapshot::Absent => Ok(()),
            Snapshot::Stale(group) => self.drop_stored(space, group),
            Snapshot::Ours(group) | Snapshot::Foreign(group) => {
                self.drop_stored(space, Some(group))
            }
        }
    }
    pub fn roster(&self, space: &str) -> Result<BTreeSet<String>, MlsError> {
        identities(&self.load(space)?)
    }
    pub fn epoch(&self, space: &str) -> Result<u64, MlsError> {
        Ok(self.load(space)?.epoch().as_u64())
    }
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

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;
    use crate::Device;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use openmls_traits::signatures::SignerError;
    use zoen_log::Signer as NativeSigner;

    struct HostSigner(NativeSigner);
    impl Signer for HostSigner {
        fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
            hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
        }
        fn signature_scheme(&self) -> openmls_traits::types::SignatureScheme {
            SUITE.signature_algorithm()
        }
    }

    #[test]
    fn external_signer_and_memory_provider_exchange_real_mls_with_native_sqlite() {
        let identity = NativeSigner::from_secret(&[1; 32]);
        let native_key = NativeSigner::from_secret(&[2; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[3; 32]));
        let mut sqlite = rusqlite::Connection::open_in_memory().unwrap();
        crate::migrate(&mut sqlite).unwrap();
        let native_cert = identity.sign(&zoen_log::device_cert_message(&native_key.id()));
        let native = Device::new(
            &sqlite,
            [4; 32],
            &identity.id(),
            native_key.secret(),
            &native_cert,
        )
        .unwrap();
        let provider = OpenMlsRustCrypto::default();
        let leaf = Leaf {
            identity: identity.id(),
            device: host.0.id(),
            cert: identity.sign(&zoen_log::device_cert_message(&host.0.id())),
        };
        let external = ExternalDevice::new(
            &provider,
            &host,
            leaf.clone(),
            hex::decode(&leaf.device).unwrap(),
        )
        .unwrap();
        let wrong = HostSigner(NativeSigner::from_secret(&[5; 32]));
        assert!(ExternalDevice::new(
            &provider,
            &wrong,
            leaf.clone(),
            hex::decode(&leaf.device).unwrap()
        )
        .is_err());
        let packages = external.key_packages(1, false).unwrap();
        assert_eq!(crate::key_package_leaf(&packages[0]).unwrap(), leaf);
        native.create_group("sp_web_interop").unwrap();
        let commit = native
            .commit("sp_web_interop", &packages, &BTreeSet::new())
            .unwrap();
        let roster = BTreeSet::from([identity.id()]);
        native
            .open("sp_web_interop", &commit.commit, &roster, native.leaf())
            .unwrap();
        assert!(external
            .join("sp_web_interop", &commit.welcome.unwrap(), &roster)
            .unwrap());
        let from_native = native.seal("sp_web_interop", b"native to browser").unwrap();
        assert_eq!(
            external
                .open("sp_web_interop", &from_native, &roster, native.leaf())
                .unwrap(),
            Opened::Application {
                plaintext: b"native to browser".to_vec(),
                from: native.leaf().clone()
            }
        );
        let from_web = external
            .seal("sp_web_interop", b"browser to native")
            .unwrap();
        assert_eq!(
            native
                .open("sp_web_interop", &from_web, &roster, &leaf)
                .unwrap(),
            Opened::Application {
                plaintext: b"browser to native".to_vec(),
                from: leaf
            }
        );
        assert_eq!(
            external.checkpoint("sp_web_interop").unwrap(),
            native.checkpoint("sp_web_interop").unwrap()
        );
    }

    fn browser(secret: [u8; 32], identity: &NativeSigner) -> (HostSigner, Leaf) {
        let host = HostSigner(NativeSigner::from_secret(&secret));
        let leaf = Leaf {
            identity: identity.id(),
            device: host.0.id(),
            cert: identity.sign(&zoen_log::device_cert_message(&host.0.id())),
        };
        (host, leaf)
    }

    fn device<'a>(
        provider: &'a OpenMlsRustCrypto,
        host: &'a HostSigner,
        leaf: &Leaf,
    ) -> ExternalDevice<'a, OpenMlsRustCrypto, HostSigner> {
        ExternalDevice::new(
            provider,
            host,
            leaf.clone(),
            hex::decode(&leaf.device).unwrap(),
        )
        .unwrap()
    }

    fn stored(provider: &OpenMlsRustCrypto) -> std::collections::HashMap<Vec<u8>, Vec<u8>> {
        provider.storage().values.read().unwrap().clone()
    }

    fn assert_credential<T: std::fmt::Debug>(result: Result<T, MlsError>) {
        assert_eq!(
            result.expect_err("snapshot was accepted").to_string(),
            "a leaf whose credential doesn't verify"
        );
    }

    fn assert_membership_wait<T>(result: Result<T, MlsError>) {
        match result {
            Err(MlsError::Mls(message)) if message == "wait for the membership commit" => {}
            Err(error) => panic!("expected the membership wait, got {error}"),
            Ok(_) => panic!("membership change was accepted"),
        }
    }

    fn admit(
        owner: &ExternalDevice<'_, OpenMlsRustCrypto, HostSigner>,
        joiner: &ExternalDevice<'_, OpenMlsRustCrypto, HostSigner>,
        space: &str,
        roster: &BTreeSet<String>,
        owner_leaf: &Leaf,
    ) {
        let packages = joiner.key_packages(1, false).unwrap();
        let commit = owner.commit(space, &packages, &BTreeSet::new()).unwrap();
        owner
            .open(space, &commit.commit, roster, owner_leaf)
            .unwrap();
        assert!(joiner
            .join(space, commit.welcome.as_deref().unwrap(), roster)
            .unwrap());
    }

    #[test]
    fn another_signers_snapshot_is_refused_at_seal_and_open() {
        let owner_identity = NativeSigner::from_secret(&[1; 32]);
        let (owner_host, owner_leaf) = browser([3; 32], &owner_identity);
        let provider = OpenMlsRustCrypto::default();
        let owner = device(&provider, &owner_host, &owner_leaf);
        let space = "sp_other_signer";
        owner.create_group(space).unwrap();
        let sealed = owner.seal(space, b"owner").unwrap();
        let roster = BTreeSet::from([owner_identity.id()]);
        let before = stored(&provider);
        let checkpoint = owner.checkpoint(space).unwrap();

        let other_identity = NativeSigner::from_secret(&[9; 32]);
        let (other_host, other_leaf) = browser([5; 32], &other_identity);
        let other = device(&provider, &other_host, &other_leaf);
        assert_credential(other.seal(space, b"stolen"));
        assert_credential(other.open(space, &sealed, &roster, &owner_leaf));
        assert_credential(other.group_state(space));
        assert_credential(other.join(space, b"not a welcome", &roster));
        assert_eq!(stored(&provider), before);
        assert_eq!(owner.checkpoint(space).unwrap(), checkpoint);
        let again = owner.seal(space, b"still ours").unwrap();
        assert_eq!(
            owner.open(space, &again, &roster, &owner_leaf).unwrap(),
            Opened::Own
        );
        other.forget(space).unwrap();
        assert!(MlsGroup::load(provider.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert!(PublicGroup::load(provider.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert_eq!(owner.group_state(space).unwrap(), GroupState::Missing);
    }

    #[test]
    fn another_device_of_the_same_account_is_refused() {
        let identity = NativeSigner::from_secret(&[1; 32]);
        let (phone_host, phone_leaf) = browser([3; 32], &identity);
        let (laptop_host, laptop_leaf) = browser([4; 32], &identity);
        assert_eq!(phone_leaf.identity, laptop_leaf.identity);
        assert_ne!(phone_leaf.device, laptop_leaf.device);
        let provider = OpenMlsRustCrypto::default();
        let phone = device(&provider, &phone_host, &phone_leaf);
        let space = "sp_same_account";
        phone.create_group(space).unwrap();
        let sealed = phone.seal(space, b"phone").unwrap();
        let roster = BTreeSet::from([identity.id()]);
        let before = stored(&provider);

        let laptop = device(&provider, &laptop_host, &laptop_leaf);
        assert_credential(laptop.seal(space, b"laptop"));
        assert_credential(laptop.open(space, &sealed, &roster, &phone_leaf));
        assert_eq!(stored(&provider), before);
        let again = phone.seal(space, b"phone again").unwrap();
        assert_eq!(
            phone.open(space, &again, &roster, &phone_leaf).unwrap(),
            Opened::Own
        );
    }

    #[test]
    fn a_snapshot_missing_its_own_leaf_is_refused() {
        let identity = NativeSigner::from_secret(&[1; 32]);
        let (host, leaf) = browser([3; 32], &identity);
        let provider = OpenMlsRustCrypto::default();
        let browser = device(&provider, &host, &leaf);
        let space = "sp_missing_leaf";
        browser.create_group(space).unwrap();
        provider
            .storage()
            .delete_own_leaf_index(&group_id(space))
            .unwrap();
        let before = stored(&provider);
        assert_membership_wait(browser.seal(space, b"missing"));
        assert_membership_wait(browser.commit(space, &[], &BTreeSet::new()));
        assert_eq!(
            browser
                .open(space, b"missing", &BTreeSet::new(), &leaf)
                .unwrap(),
            Opened::NotMember
        );
        assert_eq!(browser.group_state(space).unwrap(), GroupState::Missing);
        assert_eq!(stored(&provider), before);
        browser.forget(space).unwrap();
        assert!(MlsGroup::load(provider.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert!(PublicGroup::load(provider.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert_eq!(browser.group_state(space).unwrap(), GroupState::Missing);
    }

    #[test]
    fn an_own_leaf_outside_the_tree_is_refused() {
        let identity = NativeSigner::from_secret(&[1; 32]);
        let (host, leaf) = browser([3; 32], &identity);
        let provider = OpenMlsRustCrypto::default();
        let browser = device(&provider, &host, &leaf);
        let space = "sp_outside_tree";
        browser.create_group(space).unwrap();
        provider
            .storage()
            .write_own_leaf_index(&group_id(space), &LeafNodeIndex::new(8))
            .unwrap();
        let before = stored(&provider);
        assert_credential(browser.seal(space, b"outside"));
        assert_credential(browser.open(space, b"outside", &BTreeSet::new(), &leaf));
        assert_eq!(stored(&provider), before);
    }

    #[test]
    fn a_blank_own_leaf_is_refused() {
        let account_a = NativeSigner::from_secret(&[1; 32]);
        let account_b = NativeSigner::from_secret(&[2; 32]);
        let account_c = NativeSigner::from_secret(&[6; 32]);
        let (host_a, leaf_a) = browser([3; 32], &account_a);
        let (host_b, leaf_b) = browser([4; 32], &account_b);
        let (host_c, leaf_c) = browser([7; 32], &account_c);
        let provider_a = OpenMlsRustCrypto::default();
        let provider_b = OpenMlsRustCrypto::default();
        let provider_c = OpenMlsRustCrypto::default();
        let a = device(&provider_a, &host_a, &leaf_a);
        let b = device(&provider_b, &host_b, &leaf_b);
        let c = device(&provider_c, &host_c, &leaf_c);
        let space = "sp_blank_leaf";
        let roster = BTreeSet::from([account_a.id(), account_b.id(), account_c.id()]);
        a.create_group(space).unwrap();
        admit(&a, &b, space, &roster, &leaf_a);
        admit(&a, &c, space, &roster, &leaf_a);
        let removal = a
            .commit(space, &[], &BTreeSet::from([leaf_b.identity.clone()]))
            .unwrap();
        let remaining = BTreeSet::from([account_a.id(), account_c.id()]);
        a.open(space, &removal.commit, &remaining, &leaf_a).unwrap();
        let group = MlsGroup::load(provider_a.storage(), &group_id(space))
            .unwrap()
            .unwrap();
        let blank = group
            .treesync()
            .blank_leaves()
            .next()
            .expect("removed middle leaf stays in the tree");
        drop(group);
        provider_a
            .storage()
            .write_own_leaf_index(&group_id(space), &blank)
            .unwrap();
        let before = stored(&provider_a);
        assert_membership_wait(a.seal(space, b"blank"));
        assert_membership_wait(a.commit(space, &[], &BTreeSet::new()));
        assert_eq!(
            a.open(space, b"blank", &remaining, &leaf_a).unwrap(),
            Opened::NotMember
        );
        assert_eq!(a.group_state(space).unwrap(), GroupState::Missing);
        assert_eq!(stored(&provider_a), before);
        a.forget(space).unwrap();
        assert!(MlsGroup::load(provider_a.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert!(PublicGroup::load(provider_a.storage(), &group_id(space))
            .unwrap()
            .is_none());
        assert_eq!(a.group_state(space).unwrap(), GroupState::Missing);
        assert!(matches!(a.seal(space, b"gone"), Err(MlsError::NoGroup)));
    }

    fn leave_blank_group(provider: &OpenMlsRustCrypto, space: &str, commit: &[u8]) {
        let mut group = MlsGroup::load(provider.storage(), &group_id(space))
            .unwrap()
            .unwrap();
        let message = MlsMessageIn::tls_deserialize_exact(commit)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();
        let processed = group.process_message(provider, message).unwrap();
        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            panic!("removal was not a commit");
        };
        group.merge_staged_commit(provider, *staged).unwrap();
        let index = group.own_leaf_index().usize();
        assert!(
            group
                .treesync()
                .leaves()
                .get(index)
                .is_some_and(Option::is_none),
            "the removed device's own leaf is blank"
        );
    }

    #[test]
    fn a_removed_device_rejoins_from_a_stale_group() {
        let account_a = NativeSigner::from_secret(&[1; 32]);
        let account_b = NativeSigner::from_secret(&[2; 32]);
        let account_c = NativeSigner::from_secret(&[6; 32]);
        let (host_a, leaf_a) = browser([3; 32], &account_a);
        let (host_b, leaf_b) = browser([4; 32], &account_b);
        let (host_c, leaf_c) = browser([7; 32], &account_c);
        let provider_a = OpenMlsRustCrypto::default();
        let provider_b = OpenMlsRustCrypto::default();
        let provider_c = OpenMlsRustCrypto::default();
        let a = device(&provider_a, &host_a, &leaf_a);
        let b = device(&provider_b, &host_b, &leaf_b);
        let c = device(&provider_c, &host_c, &leaf_c);
        let space = "sp_rejoin";
        let roster = BTreeSet::from([account_a.id(), account_b.id(), account_c.id()]);
        a.create_group(space).unwrap();
        admit(&a, &b, space, &roster, &leaf_a);
        let add_c = {
            let packages = c.key_packages(1, false).unwrap();
            a.commit(space, &packages, &BTreeSet::new()).unwrap()
        };
        a.open(space, &add_c.commit, &roster, &leaf_a).unwrap();
        b.open(space, &add_c.commit, &roster, &leaf_a).unwrap();
        assert!(c
            .join(space, add_c.welcome.as_deref().unwrap(), &roster)
            .unwrap());
        let removal = a
            .commit(space, &[], &BTreeSet::from([leaf_b.identity.clone()]))
            .unwrap();
        let without_b = BTreeSet::from([account_a.id(), account_c.id()]);
        a.open(space, &removal.commit, &without_b, &leaf_a).unwrap();
        c.open(space, &removal.commit, &without_b, &leaf_a).unwrap();
        leave_blank_group(&provider_b, space, &removal.commit);
        assert_eq!(b.group_state(space).unwrap(), GroupState::Missing);
        assert_eq!(
            b.open(space, b"stale", &without_b, &leaf_a).unwrap(),
            Opened::NotMember
        );
        let packages = b.key_packages(1, false).unwrap();
        let add = a.commit(space, &packages, &BTreeSet::new()).unwrap();
        a.open(space, &add.commit, &roster, &leaf_a).unwrap();
        assert!(b
            .join(space, add.welcome.as_deref().unwrap(), &roster)
            .unwrap());
        assert_eq!(b.group_state(space).unwrap(), GroupState::Ready);
        let sealed = a.seal(space, b"back").unwrap();
        assert_eq!(
            b.open(space, &sealed, &roster, &leaf_a).unwrap(),
            Opened::Application {
                plaintext: b"back".to_vec(),
                from: leaf_a
            }
        );
    }

    #[test]
    fn the_owning_signer_accepts_its_snapshot_after_reload() {
        let identity = NativeSigner::from_secret(&[1; 32]);
        let (host, leaf) = browser([3; 32], &identity);
        let provider = OpenMlsRustCrypto::default();
        let space = "sp_reload";
        let roster = BTreeSet::from([identity.id()]);
        let (sealed, checkpoint) = {
            let browser = device(&provider, &host, &leaf);
            browser.create_group(space).unwrap();
            let sealed = browser.seal(space, b"before reload").unwrap();
            (sealed, browser.checkpoint(space).unwrap())
        };
        let saved = stored(&provider);
        let reloaded = OpenMlsRustCrypto::default();
        *reloaded.storage().values.write().unwrap() = saved;
        let browser = device(&reloaded, &host, &leaf);
        assert_eq!(browser.group_state(space).unwrap(), GroupState::Ready);
        assert_eq!(browser.checkpoint(space).unwrap(), checkpoint);
        assert_eq!(
            browser.open(space, &sealed, &roster, &leaf).unwrap(),
            Opened::Own
        );
        let again = browser.seal(space, b"after reload").unwrap();
        assert_eq!(
            browser.open(space, &again, &roster, &leaf).unwrap(),
            Opened::Own
        );
    }
}
