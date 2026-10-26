//! SQL authority coverage; uses the existing isolated PostgreSQL CI fixture.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[tokio::test]
async fn lifecycle_cas_fences_duplicates_delayed_and_legacy_controls() -> TestResult {
    let d = Database::new().await?;
    let (owner, owner_dev) = d.person("lifecycle_owner").await?;
    let (other, _) = d.person("lifecycle_other").await?;
    let (bot, _) = d.agent("lifecycle_bot", &owner, &owner_dev).await?;
    let initial = agents::control_status(&d.pool, &owner.id, &bot.id).await?;
    assert_eq!(initial.revision, 1);
    assert!(agents::control_status(&d.pool, &other.id, &bot.id)
        .await
        .is_err());
    let device = owner_dev.device.as_deref().ok_or("device")?;
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let pause = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Pause,
        &a,
        1,
    )
    .await?;
    assert_eq!((pause.status.as_str(), pause.revision), ("paused", 2));
    assert!(agents::invite_live(&d.pool, &bot.id).await?);
    let duplicate = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Pause,
        &a,
        1,
    )
    .await?;
    assert_eq!(duplicate, pause);
    assert!(agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &a,
        1
    )
    .await
    .is_err());
    let delayed = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &b,
        1,
    )
    .await?;
    assert_eq!(delayed, pause);
    let resume = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &b,
        2,
    )
    .await?;
    assert_eq!((resume.status.as_str(), resume.revision), ("active", 3));
    let late = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Pause,
        &a,
        1,
    )
    .await?;
    assert_eq!(late, resume);
    agents::control(&d.pool, &owner.id, &bot.id, AgentAction::Pause).await?;
    let legacy = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &b,
        2,
    )
    .await?;
    assert_eq!((legacy.status.as_str(), legacy.revision), ("paused", 4));
    assert!(legacy.operation.is_empty());
    assert!(agents::control_reviewed(
        &d.pool,
        &other.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &b,
        4
    )
    .await
    .is_err());
    d.finish().await;
    Ok(())
}
#[tokio::test]
async fn lifecycle_racing_requests_have_one_winner_and_revocation_is_final() -> TestResult {
    let d = Database::new().await?;
    let (owner, owner_dev) = d.person("lifecycle_race_owner").await?;
    let (bot, bot_dev) = d.agent("lifecycle_race_bot", &owner, &owner_dev).await?;
    let device = owner_dev.device.as_deref().ok_or("device")?;
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let (one, two) = tokio::join!(
        agents::control_reviewed(
            &d.pool,
            &owner.id,
            device,
            &bot.id,
            AgentAction::Pause,
            &a,
            1
        ),
        agents::control_reviewed(
            &d.pool,
            &owner.id,
            device,
            &bot.id,
            AgentAction::Resume,
            &b,
            1
        )
    );
    assert_eq!(one?, two?);
    let revoked = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Revoke,
        &"c".repeat(64),
        2,
    )
    .await?;
    assert_eq!((revoked.status.as_str(), revoked.revision), ("revoked", 3));
    let runtime_device = bot_dev.device.as_deref().ok_or("device")?;
    let (disabled,): (bool,) =
        sqlx::query_as("SELECT revoked_at IS NOT NULL FROM devices WHERE device=$1")
            .bind(runtime_device)
            .fetch_one(&d.pool)
            .await?;
    assert!(disabled);
    assert!(!agents::invite_live(&d.pool, &bot.id).await?);
    let late = agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Resume,
        &a,
        3,
    )
    .await?;
    assert_eq!(late, revoked);
    d.finish().await;
    Ok(())
}

#[tokio::test]
async fn lifecycle_sql_revocation_fences_stale_link_when_physical_cleanup_fails() -> TestResult {
    let d = Database::new().await?;
    let (owner, owner_dev) = d.person("lifecycle_link_owner").await?;
    let (bot, _) = d.agent("lifecycle_link_bot", &owner, &owner_dev).await?;
    let stale_link_agent = bot.id.clone(); // The exact old link still points here.
    assert!(agents::invite_live(&d.pool, &stale_link_agent).await?);
    assert!(!agents::invite_live(&d.pool, &"0".repeat(64)).await?);
    let mut rollback = d.pool.begin().await?;
    sqlx::query("UPDATE agent_citizens SET revoked_at=now() WHERE agent=$1")
        .bind(&bot.id)
        .execute(&mut *rollback)
        .await?;
    rollback.rollback().await?;
    assert!(agents::invite_live(&d.pool, &stale_link_agent).await?);
    let device = owner_dev.device.as_deref().ok_or("device")?;
    agents::control_reviewed(
        &d.pool,
        &owner.id,
        device,
        &bot.id,
        AgentAction::Revoke,
        &"d".repeat(64),
        1,
    )
    .await?;
    // No FDB mutation is performed in this isolated SQL test: the old link is
    // deliberately retained, exactly as after a failed cleanup/crash boundary.
    assert!(!agents::invite_live(&d.pool, &stale_link_agent).await?);
    assert!(
        agents::control(&d.pool, &owner.id, &bot.id, AgentAction::Resume)
            .await
            .is_err()
    );
    assert!(!agents::invite_live(&d.pool, &stale_link_agent).await?);
    d.finish().await;
    Ok(())
}
