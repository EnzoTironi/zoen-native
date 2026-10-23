use super::*;
use protocol::DeviceRecord;

fn engine() -> Engine {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Device owner", "device_owner", "http://127.0.0.1:1")
        .unwrap();
    engine
}
fn record(device: &str, revoked: bool) -> DeviceRecord {
    DeviceRecord {
        device: device.into(),
        revoked,
    }
}
#[test]
fn accepted_directory_and_revocation_keep_the_captured_current_device() {
    let mut engine = engine();
    let owner = DirectoryOwner::capture(&engine).unwrap();
    let other = Signer::from_secret(&[71; 32]).id();
    let list = owner
        .list(
            &mut engine,
            vec![record(&owner.device, false), record(&other, false)],
        )
        .unwrap();
    assert!(list[0].this_device);
    assert!(!list[1].this_device);
    owner.unlinked(&mut engine, &other).unwrap();
    assert!(engine.revoked_devices().contains(&other));
    assert!(!engine.revoked_devices().contains(&owner.device));
}
#[test]
fn late_old_owner_directory_or_unlink_ack_cannot_change_the_replacement_cache() {
    let old = engine();
    let owner = DirectoryOwner::capture(&old).unwrap();
    let target = Signer::from_secret(&[72; 32]).id();
    let mut replacement = engine();
    let current = DirectoryOwner::capture(&replacement).unwrap();
    current
        .list(&mut replacement, vec![record(&current.device, false)])
        .unwrap();
    let before =
        ["devices.active", "devices.revoked"].map(|key| replacement.store.meta(key).unwrap());
    assert!(owner
        .list(&mut replacement, vec![record(&target, true)])
        .is_err());
    assert!(owner.unlinked(&mut replacement, &target).is_err());
    assert_eq!(
        ["devices.active", "devices.revoked"].map(|key| replacement.store.meta(key).unwrap()),
        before
    );
    assert!(!replacement.revoked_devices().contains(&target));
}
#[test]
fn relay_replacement_or_revoked_device_refuses_its_old_directory_completion() {
    let mut engine = engine();
    let owner = DirectoryOwner::capture(&engine).unwrap();
    engine.net.account.as_mut().unwrap().relay_url = "http://127.0.0.1:2".into();
    assert!(owner
        .list(&mut engine, vec![record(&owner.device, false)])
        .is_err());
    assert!(engine.store.meta("devices.active").unwrap().is_none());
    engine.net.account.as_mut().unwrap().relay_url = owner.relay.clone();
    engine
        .store
        .set_meta(
            "devices.revoked",
            &serde_json::to_string(&vec![&owner.device]).unwrap(),
        )
        .unwrap();
    assert!(owner
        .unlinked(&mut engine, &Signer::from_secret(&[73; 32]).id())
        .is_err());
    assert!(engine.store.meta("devices.active").unwrap().is_none());
}
#[test]
fn corrupt_revocation_cache_is_an_error_not_an_empty_authorization_set() {
    let mut engine = engine();
    let owner = DirectoryOwner::capture(&engine).unwrap();
    engine
        .store
        .set_meta("devices.revoked", "not-json")
        .unwrap();
    assert!(matches!(
        owner.list(&mut engine, vec![record(&owner.device, false)]),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(
        engine.store.meta("devices.revoked").unwrap().as_deref(),
        Some("not-json")
    );
    assert!(engine.store.meta("devices.active").unwrap().is_none());
}
