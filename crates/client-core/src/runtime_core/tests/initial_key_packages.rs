//! The public facade outbox survives complete images without replacing its keys.
use super::*;

#[test]
fn initial_package_outbox_reopens_exactly_and_acknowledges_only_its_digest() {
    let mut f = Fixture::new();
    assert!(f.core.initial_key_package_publication().unwrap().is_none());
    let (first, image) = f.core.prepare_initial_key_packages().unwrap();
    assert!(!first.acknowledged());
    assert_eq!(first.packages().len(), 32);
    for package in first.packages() {
        let leaf = ::mls::key_package_leaf(package).unwrap();
        assert_eq!(leaf.identity, f.agent_profile.id);
        assert_eq!(leaf.device, f.agent_device.id());
    }
    let mut cold = DeviceCore::restore(image, f.credential()).unwrap();
    let (retry, retained) = cold.prepare_initial_key_packages().unwrap();
    assert_eq!(retry.packages(), first.packages());
    assert_eq!(retry.digest(), first.digest());
    let mut wrong = DeviceCore::restore(retained, f.credential()).unwrap();
    assert!(wrong
        .acknowledge_initial_key_packages("another-batch")
        .is_err());
    assert!(wrong.image().is_err());
    let ack = cold
        .acknowledge_initial_key_packages(first.digest())
        .unwrap();
    let mut reopened = DeviceCore::restore(ack, f.credential()).unwrap();
    let (published, _) = reopened.prepare_initial_key_packages().unwrap();
    assert!(published.acknowledged());
    assert_eq!(published.packages(), first.packages());
    assert_eq!(published.digest(), first.digest());
}

#[test]
fn malformed_initial_outbox_refuses_instead_of_generating_replacement_keys() {
    let mut f = Fixture::new();
    let key = "runtime.initial-key-packages/1";
    f.core
        .engine
        .store
        .set_meta(key, "{\"format\":99}")
        .unwrap();
    assert!(f.core.initial_key_package_publication().is_err());
    assert!(f.core.prepare_initial_key_packages().is_err());
    assert_eq!(
        f.core.engine.store.meta(key).unwrap().as_deref(),
        Some("{\"format\":99}")
    );
    assert!(f.core.image().is_err());
}
