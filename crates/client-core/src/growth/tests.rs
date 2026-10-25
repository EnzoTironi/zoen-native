use super::*;

#[test]
fn onboarding_never_exposes_an_unpersisted_unit_and_retry_keeps_its_assignment(
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::open(":memory:".into(), "en".into())?;
    client.lock().store.conn().execute_batch(
        "CREATE TRIGGER deny_growth_unit BEFORE INSERT ON meta
         WHEN NEW.key = 'growth:unit'
         BEGIN SELECT RAISE(ABORT, 'injected growth unit write failure'); END;",
    )?;
    assert!(matches!(
        client.growth_onboarding_plan(),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(client.lock().store.meta(META_UNIT)?, None);
    assert_eq!(client.lock().store.meta(META_EXPOSURES)?, None);
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER deny_growth_unit")?;
    let first = client.growth_onboarding_plan()?;
    let unit = client
        .lock()
        .store
        .meta(META_UNIT)?
        .ok_or("unit not retained")?;
    assert_eq!(unit.len(), 32);
    assert!(unit.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(client.growth_onboarding_plan()?, first);
    assert_eq!(client.lock().store.meta(META_UNIT)?, Some(unit));
    Ok(())
}
