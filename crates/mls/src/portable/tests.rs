use super::*;
use crate::Device;
use event_log::Signer as NativeSigner;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::SignerError;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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
fn external_signer_and_memory_provider_exchange_real_mls_with_native_sqlite() -> TestResult {
    let identity = NativeSigner::from_secret(&[1; 32]);
    let native_key = NativeSigner::from_secret(&[2; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[3; 32]));
    let mut sqlite = rusqlite::Connection::open_in_memory()?;
    crate::migrate(&mut sqlite)?;
    let native_cert = identity.sign(&event_log::device_cert_message(&native_key.id()));
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
        cert: identity.sign(&event_log::device_cert_message(&host.0.id())),
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
    Ok(())
}

fn browser(secret: [u8; 32], identity: &NativeSigner) -> (HostSigner, Leaf) {
    let host = HostSigner(NativeSigner::from_secret(&secret));
    let leaf = Leaf {
        identity: identity.id(),
        device: host.0.id(),
        cert: identity.sign(&event_log::device_cert_message(&host.0.id())),
    };
    (host, leaf)
}

fn device<'a>(
    provider: &'a OpenMlsRustCrypto,
    host: &'a HostSigner,
    leaf: &Leaf,
) -> TestResult<ExternalDevice<'a, OpenMlsRustCrypto, HostSigner>> {
    Ok(ExternalDevice::new(
        provider,
        host,
        leaf.clone(),
        hex::decode(&leaf.device)?,
    )?)
}

fn stored(provider: &OpenMlsRustCrypto) -> TestResult<std::collections::HashMap<Vec<u8>, Vec<u8>>> {
    Ok(provider
        .storage()
        .values
        .read()
        .map_err(|_| "fixture provider storage lock poisoned")?
        .clone())
}

fn assert_credential<T>(result: Result<T, MlsError>) {
    assert!(
        matches!(result.err(), Some(MlsError::Credential)),
        "snapshot must refuse invalid credentials"
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
) -> TestResult {
    let packages = joiner.key_packages(1, false)?;
    let commit = owner.commit(space, &packages, &BTreeSet::new())?;
    owner.open(space, &commit.commit, roster, owner_leaf)?;
    assert!(joiner.join(
        space,
        commit.welcome.as_deref().ok_or("fixture welcome missing")?,
        roster
    )?);
    Ok(())
}

#[test]
fn another_signers_snapshot_is_refused_at_seal_and_open() -> TestResult {
    let owner_identity = NativeSigner::from_secret(&[1; 32]);
    let (owner_host, owner_leaf) = browser([3; 32], &owner_identity);
    let provider = OpenMlsRustCrypto::default();
    let owner = device(&provider, &owner_host, &owner_leaf)?;
    let space = "sp_other_signer";
    owner.create_group(space).unwrap();
    let sealed = owner.seal(space, b"owner").unwrap();
    let roster = BTreeSet::from([owner_identity.id()]);
    let before = stored(&provider)?;
    let checkpoint = owner.checkpoint(space).unwrap();

    let other_identity = NativeSigner::from_secret(&[9; 32]);
    let (other_host, other_leaf) = browser([5; 32], &other_identity);
    let other = device(&provider, &other_host, &other_leaf)?;
    assert_credential(other.seal(space, b"stolen"));
    assert_credential(other.open(space, &sealed, &roster, &owner_leaf));
    assert_credential(other.group_state(space));
    assert_credential(other.join(space, b"not a welcome", &roster));
    assert_eq!(stored(&provider)?, before);
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
    Ok(())
}

#[test]
fn another_device_of_the_same_account_is_refused() -> TestResult {
    let identity = NativeSigner::from_secret(&[1; 32]);
    let (phone_host, phone_leaf) = browser([3; 32], &identity);
    let (laptop_host, laptop_leaf) = browser([4; 32], &identity);
    assert_eq!(phone_leaf.identity, laptop_leaf.identity);
    assert_ne!(phone_leaf.device, laptop_leaf.device);
    let provider = OpenMlsRustCrypto::default();
    let phone = device(&provider, &phone_host, &phone_leaf)?;
    let space = "sp_same_account";
    phone.create_group(space).unwrap();
    let sealed = phone.seal(space, b"phone").unwrap();
    let roster = BTreeSet::from([identity.id()]);
    let before = stored(&provider)?;

    let laptop = device(&provider, &laptop_host, &laptop_leaf)?;
    assert_credential(laptop.seal(space, b"laptop"));
    assert_credential(laptop.open(space, &sealed, &roster, &phone_leaf));
    assert_eq!(stored(&provider)?, before);
    let again = phone.seal(space, b"phone again").unwrap();
    assert_eq!(
        phone.open(space, &again, &roster, &phone_leaf).unwrap(),
        Opened::Own
    );
    Ok(())
}

#[test]
fn a_snapshot_missing_its_own_leaf_is_refused() -> TestResult {
    let identity = NativeSigner::from_secret(&[1; 32]);
    let (host, leaf) = browser([3; 32], &identity);
    let provider = OpenMlsRustCrypto::default();
    let browser = device(&provider, &host, &leaf)?;
    let space = "sp_missing_leaf";
    browser.create_group(space).unwrap();
    provider
        .storage()
        .delete_own_leaf_index(&group_id(space))
        .unwrap();
    let before = stored(&provider)?;
    assert_membership_wait(browser.seal(space, b"missing"));
    assert_membership_wait(browser.commit(space, &[], &BTreeSet::new()));
    assert_eq!(
        browser
            .open(space, b"missing", &BTreeSet::new(), &leaf)
            .unwrap(),
        Opened::NotMember
    );
    assert_eq!(browser.group_state(space).unwrap(), GroupState::Missing);
    assert_eq!(stored(&provider)?, before);
    browser.forget(space).unwrap();
    assert!(MlsGroup::load(provider.storage(), &group_id(space))
        .unwrap()
        .is_none());
    assert!(PublicGroup::load(provider.storage(), &group_id(space))
        .unwrap()
        .is_none());
    assert_eq!(browser.group_state(space).unwrap(), GroupState::Missing);
    Ok(())
}

#[test]
fn an_own_leaf_outside_the_tree_is_refused() -> TestResult {
    let identity = NativeSigner::from_secret(&[1; 32]);
    let (host, leaf) = browser([3; 32], &identity);
    let provider = OpenMlsRustCrypto::default();
    let browser = device(&provider, &host, &leaf)?;
    let space = "sp_outside_tree";
    browser.create_group(space).unwrap();
    provider
        .storage()
        .write_own_leaf_index(&group_id(space), &LeafNodeIndex::new(8))
        .unwrap();
    let before = stored(&provider)?;
    assert_credential(browser.seal(space, b"outside"));
    assert_credential(browser.open(space, b"outside", &BTreeSet::new(), &leaf));
    assert_eq!(stored(&provider)?, before);
    Ok(())
}

#[test]
fn a_blank_own_leaf_is_refused() -> TestResult {
    let account_a = NativeSigner::from_secret(&[1; 32]);
    let account_b = NativeSigner::from_secret(&[2; 32]);
    let account_c = NativeSigner::from_secret(&[6; 32]);
    let (host_a, leaf_a) = browser([3; 32], &account_a);
    let (host_b, leaf_b) = browser([4; 32], &account_b);
    let (host_c, leaf_c) = browser([7; 32], &account_c);
    let provider_a = OpenMlsRustCrypto::default();
    let provider_b = OpenMlsRustCrypto::default();
    let provider_c = OpenMlsRustCrypto::default();
    let a = device(&provider_a, &host_a, &leaf_a)?;
    let b = device(&provider_b, &host_b, &leaf_b)?;
    let c = device(&provider_c, &host_c, &leaf_c)?;
    let space = "sp_blank_leaf";
    let roster = BTreeSet::from([account_a.id(), account_b.id(), account_c.id()]);
    a.create_group(space).unwrap();
    admit(&a, &b, space, &roster, &leaf_a)?;
    admit(&a, &c, space, &roster, &leaf_a)?;
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
    let before = stored(&provider_a)?;
    assert_membership_wait(a.seal(space, b"blank"));
    assert_membership_wait(a.commit(space, &[], &BTreeSet::new()));
    assert_eq!(
        a.open(space, b"blank", &remaining, &leaf_a).unwrap(),
        Opened::NotMember
    );
    assert_eq!(a.group_state(space).unwrap(), GroupState::Missing);
    assert_eq!(stored(&provider_a)?, before);
    a.forget(space).unwrap();
    assert!(MlsGroup::load(provider_a.storage(), &group_id(space))
        .unwrap()
        .is_none());
    assert!(PublicGroup::load(provider_a.storage(), &group_id(space))
        .unwrap()
        .is_none());
    assert_eq!(a.group_state(space).unwrap(), GroupState::Missing);
    assert!(matches!(a.seal(space, b"gone"), Err(MlsError::NoGroup)));
    Ok(())
}

fn leave_blank_group(provider: &OpenMlsRustCrypto, space: &str, commit: &[u8]) -> TestResult {
    let mut group =
        MlsGroup::load(provider.storage(), &group_id(space))?.ok_or("fixture group missing")?;
    let message = MlsMessageIn::tls_deserialize_exact(commit)?
        .try_into_protocol_message()
        .map_err(|error| format!("fixture protocol message: {error:?}"))?;
    let processed = group.process_message(provider, message)?;
    let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
        panic!("removal was not a commit");
    };
    group.merge_staged_commit(provider, *staged)?;
    let index = group.own_leaf_index().usize();
    assert!(
        group
            .treesync()
            .leaves()
            .get(index)
            .is_some_and(Option::is_none),
        "the removed device's own leaf is blank"
    );
    Ok(())
}

#[test]
fn a_removed_device_rejoins_from_a_stale_group() -> TestResult {
    let account_a = NativeSigner::from_secret(&[1; 32]);
    let account_b = NativeSigner::from_secret(&[2; 32]);
    let account_c = NativeSigner::from_secret(&[6; 32]);
    let (host_a, leaf_a) = browser([3; 32], &account_a);
    let (host_b, leaf_b) = browser([4; 32], &account_b);
    let (host_c, leaf_c) = browser([7; 32], &account_c);
    let provider_a = OpenMlsRustCrypto::default();
    let provider_b = OpenMlsRustCrypto::default();
    let provider_c = OpenMlsRustCrypto::default();
    let a = device(&provider_a, &host_a, &leaf_a)?;
    let b = device(&provider_b, &host_b, &leaf_b)?;
    let c = device(&provider_c, &host_c, &leaf_c)?;
    let space = "sp_rejoin";
    let roster = BTreeSet::from([account_a.id(), account_b.id(), account_c.id()]);
    a.create_group(space).unwrap();
    admit(&a, &b, space, &roster, &leaf_a)?;
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
    leave_blank_group(&provider_b, space, &removal.commit)?;
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
    Ok(())
}

#[test]
fn the_owning_signer_accepts_its_snapshot_after_reload() -> TestResult {
    let identity = NativeSigner::from_secret(&[1; 32]);
    let (host, leaf) = browser([3; 32], &identity);
    let provider = OpenMlsRustCrypto::default();
    let space = "sp_reload";
    let roster = BTreeSet::from([identity.id()]);
    let (sealed, checkpoint) = {
        let browser = device(&provider, &host, &leaf)?;
        browser.create_group(space).unwrap();
        let sealed = browser.seal(space, b"before reload").unwrap();
        (sealed, browser.checkpoint(space).unwrap())
    };
    let saved = stored(&provider)?;
    let reloaded = OpenMlsRustCrypto::default();
    *reloaded.storage().values.write().unwrap() = saved;
    let browser = device(&reloaded, &host, &leaf)?;
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
    Ok(())
}
