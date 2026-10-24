use super::*;
use event_log::{Author, Signer};

fn fixture() -> Engine {
    let identity = Signer::from_secret(&[71; 32]);
    let mut engine = Engine::open(":memory:").unwrap();
    engine.me = Some(identity.id());
    engine.store.set_meta("me", &identity.id()).unwrap();
    engine.net.author = Some(Author::device(&identity, Signer::from_secret(&[72; 32])));
    engine
        .init_profile(&identity.id(), "private-name-canary")
        .unwrap();
    engine.net.profiles.supported = true;
    engine.net.profiles.shares_dirty = false;
    let public = engine.net.profiles.agreement.as_ref().unwrap().public_hex();
    engine.agreement_published(&public);
    engine.profile_upload_answered(1, Ok(()));
    assert_eq!(engine.profile_settlement(), ProfileSettlement::Ready);
    engine
}

#[test]
fn actual_profile_gates_keep_original_short_circuit_order() {
    let mut engine = fixture();
    engine
        .store
        .set_meta(META_OWN_SYNC, "corrupt-private-journal-canary")
        .unwrap();
    engine
        .net
        .profiles
        .need_agreement
        .insert("private-id-canary".into());
    engine
        .net
        .profiles
        .need_profiles
        .insert("private-id-canary".into());
    engine.net.profiles.shares_dirty = true;
    engine.net.profiles.supported = false;
    assert_eq!(engine.profile_settlement(), ProfileSettlement::NotRequired);
    assert!(engine.profiles_settled());
    engine.net.profiles.supported = true;
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::AgreementKeys
    );
    engine.net.profiles.need_agreement.clear();
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::SealedProfiles
    );
    engine.net.profiles.need_profiles.clear();
    assert_eq!(engine.profile_settlement(), ProfileSettlement::KeyShares);
    engine.net.profiles.shares_dirty = false;
    engine
        .store
        .set_meta(META_AGREEMENT, "unpublished")
        .unwrap();
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::AgreementPublication
    );
    let public = engine.net.profiles.agreement.as_ref().unwrap().public_hex();
    engine.agreement_published(&public);
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::JournalUnavailable
    );
    assert!(!engine.profiles_settled());
}

#[test]
fn actual_publication_and_key_failures_stay_unsettled_and_recover_only_after_original_transition() {
    let mut engine = fixture();
    let mut journal = engine.own_profile_journal().unwrap();
    journal.sync.pending = Some(2);
    engine.save_own_profile_journal(&journal).unwrap();
    engine.net.profiles.own_refreshed = true;
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::ProfileUpload
    );
    engine.net.profiles.own_refreshed = false;
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::PublicationIntent
    );
    journal.sync.cannot_open(3);
    engine.save_own_profile_journal(&journal).unwrap();
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::PublicationIntent
    );
    journal.sync.pending = None;
    engine.save_own_profile_journal(&journal).unwrap();
    assert_eq!(
        engine.profile_settlement(),
        ProfileSettlement::ProfileKeyUnavailable
    );
    assert!(!engine.profiles_settled());
    assert_eq!(engine.own_profile_journal().unwrap().sync, journal.sync);
    let bytes = serde_json::to_string(&engine.profile_settlement()).unwrap();
    assert_eq!(bytes, "\"profile_key_unavailable\"");
    assert!(!bytes.contains("canary"));
}
