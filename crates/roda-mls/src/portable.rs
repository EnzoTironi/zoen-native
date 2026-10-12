//! OpenMLS groups over a caller-owned provider and signer. Private device keys stay
//! in the host. The host must commit the complete provider snapshot before publishing.
use std::collections::BTreeSet;

use openmls::prelude::{tls_codec::Deserialize as _, tls_codec::Serialize as _, *};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::signatures::Signer;
use sha2::{Digest, Sha256};

use crate::group::*;
use crate::{leaf_of, mls, validate_key_package, Leaf, MlsError, SUITE};

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
        if !roda_log::verify_sig(&leaf.device, &leaf.encode(), &hex::encode(proof)) {
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

    fn load(&self, space: &str) -> Result<MlsGroup, MlsError> {
        let group = MlsGroup::load(self.provider.storage(), &group_id(space))
            .map_err(storage_error)?
            .ok_or(MlsError::NoGroup)?;
        if group.ciphersuite() != SUITE {
            return Err(MlsError::Credential);
        }
        Ok(group)
    }

    pub fn group_state(&self, space: &str) -> Result<GroupState, MlsError> {
        match self.load(space) {
            Ok(group) if group.pending_commit().is_some() => Ok(GroupState::Pending),
            Ok(group) if group.is_active() => Ok(GroupState::Ready),
            Ok(_) | Err(MlsError::NoGroup) => Ok(GroupState::Missing),
            Err(error) => Err(error),
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
        if self.group_state(space)? != GroupState::Missing {
            return Err(MlsError::Mls("group already exists".into()));
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
        let mut group = self.load(space)?;
        if !group.is_active() || group.pending_commit().is_some() {
            return Err(MlsError::Mls("wait for the membership commit".into()));
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
        let mut group = self.load(space)?;
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
        let mut group = match self.load(space) {
            Ok(group) if group.is_active() => group,
            Ok(_) | Err(MlsError::NoGroup) => return Ok(Opened::NotMember),
            Err(error) => return Err(error),
        };
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
        match self.load(space) {
            Ok(mut group) => group.delete(self.provider.storage()).map_err(storage_error),
            Err(MlsError::NoGroup) => Ok(()),
            Err(error) => Err(error),
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
    use roda_log::Signer as NativeSigner;

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
        let native_cert = identity.sign(&roda_log::device_cert_message(&native_key.id()));
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
            cert: identity.sign(&roda_log::device_cert_message(&host.0.id())),
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
}
