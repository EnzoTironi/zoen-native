//! An unknown durable marker cannot produce a successful integrity certificate.
use super::*;

#[test]
fn unreadable_marker_is_a_storage_error_instead_of_a_valid_audit(
) -> Result<(), Box<dyn std::error::Error>> {
    let core = Client::open(":memory:".into(), "en".into())?;
    core.seed_demo_if_empty()?;
    let space = core
        .client_snapshot()
        .conversations
        .first()
        .ok_or("missing fixture conversation")?
        .id
        .clone();
    let healthy: serde_json::Value = serde_json::from_str(&core.client_verify_log(space.clone())?)?;
    assert_eq!(healthy["valid"], true);
    // This is the existing durable key contract, not an exposed production helper.
    let marker = format!("mls.fork:{space}");
    {
        let engine = core.lock();
        engine.store.conn().execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![marker, vec![0xff_u8]],
        )?;
    }
    assert!(matches!(
        core.client_verify_log(space.clone()),
        Err(CoreError::Storage { .. })
    ));
    assert!(
        core.client_integrity_warnings().is_empty(),
        "storage error is not evidence of a cryptographic fork"
    );
    core.lock().store.meta_delete(&marker)?;
    let recovered: serde_json::Value = serde_json::from_str(&core.client_verify_log(space)?)?;
    assert_eq!(recovered["valid"], true);
    Ok(())
}
