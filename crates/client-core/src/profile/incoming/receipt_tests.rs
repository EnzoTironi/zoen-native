//! Real SQLite faults must not acknowledge an uncommitted authenticated receipt.
use super::*;
use event_log::{Author, Signer};

fn fixture() -> R<(Engine, Signer)> {
    let identity = Signer::from_secret(&[71; 32]);
    let mut engine = Engine::open(":memory:")?;
    engine.me = Some(identity.id());
    engine.store.set_meta("me", &identity.id())?;
    engine.net.author = Some(Author::device(&identity, Signer::from_secret(&[72; 32])));
    engine.init_profile(&identity.id(), "Original")?;
    engine.profiles_session_start(true);
    engine.sealed_profiles_arrived(Vec::new(), &[identity.id()]);
    let upload = engine.profile_to_upload().expect("bootstrap upload");
    engine.profile_upload_answered(upload.version, Ok(()));
    let (public, _) = engine
        .agreement_to_publish()
        .expect("agreement publication");
    engine.agreement_published(&public);
    engine.profile_share_pass()?;
    assert!(engine.profiles_settled());
    Ok((engine, identity))
}

fn remote(engine: &Engine, identity: &Signer, version: u64) -> R<SealedProfile> {
    let author = Author::device(identity, Signer::from_secret(&[92; 32]));
    let ciphertext = seal_profile(
        &engine.my_fields(&identity.id())?.0,
        &identity.id(),
        version,
        &ProfileFields {
            name: "Other device".into(),
            ..Default::default()
        },
    );
    Ok(SealedProfile {
        identity: identity.id(),
        version,
        signed: sign_upload(&author, version, &ciphertext).expect("certified signed upload"),
        ciphertext,
    })
}

fn assert_pending(engine: &Engine, asked: &[IdentityId]) {
    assert!(
        !engine.profiles_answer_complete(asked),
        "uncommitted receipt stays pending"
    );
    assert!(!engine.net.profiles.own_refreshed);
    assert!(!engine.profiles_settled());
}

#[test]
fn own_fields_write_fault_retains_lookup_until_same_signed_receipt_commits() -> R<()> {
    let (mut engine, identity) = fixture()?;
    let incoming = remote(&engine, &identity, 2)?;
    let before = engine.my_fields(&identity.id())?;
    let journal = engine.store.meta(META_OWN_SYNC)?;
    engine.profile_changed(&identity.id());
    let asked = engine.take_need_profiles();
    engine.store.raw_execute("CREATE TEMP TRIGGER fail_fields BEFORE UPDATE OF fields ON profile_keys BEGIN SELECT RAISE(ABORT, 'fixture fields fault'); END")?;
    assert!(engine
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    assert_eq!(engine.my_fields(&identity.id())?.2, before.2);
    assert_eq!(engine.my_fields(&identity.id())?.3, before.3);
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, journal);
    assert_pending(&engine, &asked);
    engine.store.raw_execute("DROP TRIGGER fail_fields")?;
    let retry = engine.take_need_profiles();
    assert_eq!(retry, asked);
    assert_eq!(
        engine.sealed_profiles_arrived(vec![incoming], &retry),
        asked
    );
    assert_eq!(engine.my_fields(&identity.id())?.3.name, "Other device");
    assert_eq!(engine.own_profile_journal()?.sync.published, 2);
    assert!(engine.profiles_answer_complete(&retry));
    assert!(engine.profiles_settled());
    Ok(())
}

#[test]
fn own_journal_write_fault_preserves_pending_fields_and_exact_ciphertext() -> R<()> {
    let (mut engine, identity) = fixture()?;
    engine.update_my_profile("Local pending", "bio", PhotoChange::Keep)?;
    let retained = engine.profile_to_upload().expect("retained pending upload");
    let incoming = remote(&engine, &identity, 3)?;
    let before = engine.my_fields(&identity.id())?;
    let journal = engine.store.meta(META_OWN_SYNC)?;
    engine.profile_changed(&identity.id());
    let asked = engine.take_need_profiles();
    engine.store.raw_execute("CREATE TEMP TRIGGER fail_journal BEFORE INSERT ON meta WHEN NEW.key = 'profile:own-sync:v1' BEGIN SELECT RAISE(ABORT, 'fixture journal fault'); END")?;
    assert!(engine
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    assert_eq!(engine.my_fields(&identity.id())?.2, before.2);
    assert_eq!(engine.my_fields(&identity.id())?.3, before.3);
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, journal);
    assert_eq!(
        engine
            .own_profile_journal()?
            .upload
            .expect("original ciphertext")
            .sealed(&identity.id()),
        retained
    );
    assert_pending(&engine, &asked);
    engine.store.raw_execute("DROP TRIGGER fail_journal")?;
    let retry = engine.take_need_profiles();
    assert_eq!(retry, asked);
    engine.sealed_profiles_arrived(vec![incoming], &retry);
    assert_eq!(engine.my_fields(&identity.id())?.3, before.3);
    let recovered = engine.profile_to_upload().expect("rebased local upload");
    assert_eq!(recovered.version, 4);
    assert_ne!(recovered, retained);
    engine.profile_upload_answered(retained.version, Ok(()));
    assert_eq!(engine.own_profile_journal()?.sync.pending, Some(4));
    engine.profile_upload_answered(recovered.version, Ok(()));
    engine.profile_share_pass()?;
    assert!(engine.profiles_settled());
    Ok(())
}

#[test]
fn agreement_write_fault_retains_demand_without_negative_cache_until_retry() -> R<()> {
    let (mut engine, identity) = fixture()?;
    let mut peer = Engine::open(":memory:")?;
    peer.create_account("Peer", "peer", "http://relay.test")?;
    peer.net.profiles.supported = true;
    let peer_id = peer.me_id()?;
    let (public, signed) = peer
        .agreement_to_publish()
        .expect("real signed peer agreement");
    let record = AgreementKeyRecord {
        identity: peer_id.clone(),
        public: public.clone(),
        signed,
    };
    let fields = engine.my_fields(&identity.id())?.3;
    let journal = engine.store.meta(META_OWN_SYNC)?;
    engine.net.profiles.need_agreement.insert(peer_id.clone());
    let asked = engine.take_need_agreement();
    engine.store.raw_execute("CREATE TEMP TRIGGER fail_agreement BEFORE INSERT ON peer_agreement_keys BEGIN SELECT RAISE(ABORT, 'fixture agreement fault'); END")?;
    engine.agreement_keys_arrived(vec![record.clone()], &asked);
    assert_eq!(engine.store.peer_agreement_key(&peer_id)?, None);
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, journal);
    assert_eq!(engine.my_fields(&identity.id())?.3, fields);
    assert_eq!(engine.outbox_len(), 0);
    assert!(!engine.net.profiles.no_agreement.contains(&peer_id));
    assert_eq!(
        engine.take_need_agreement(),
        asked,
        "failed persistence is still work"
    );
    assert!(!engine.profiles_settled());
    engine.store.raw_execute("DROP TRIGGER fail_agreement")?;
    let retry = engine.take_need_agreement();
    engine.agreement_keys_arrived(vec![record], &retry);
    assert_eq!(engine.store.peer_agreement_key(&peer_id)?, Some(public));
    assert!(engine.take_need_agreement().is_empty());
    assert!(!engine.net.profiles.no_agreement.contains(&peer_id));
    engine.profile_share_pass()?;
    assert!(engine.profiles_settled());
    Ok(())
}
