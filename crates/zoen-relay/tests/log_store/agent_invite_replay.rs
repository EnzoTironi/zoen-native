//! The real FDB boundary: preflight never spends quota, and public preview does
//! not acquire the repeat-redeemer exception.
use super::*;

pub(super) async fn agent_redemption_preflight_preserves_idempotent_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let log = store()?;
    let owner = Author::root(Signer::generate()?);
    let first = Author::root(Signer::generate()?);
    let other = Author::root(Signer::generate()?);
    let agent = types::new_id("agent")?;
    let invite = log
        .create_agent_invite(&owner.identity, &agent, 1, 3600)
        .await?;
    for _ in 0..3 {
        log.preview_agent_invite_for_redeemer(&invite.code, &first.identity)
            .await?;
        log.preview_agent_invite_for_redeemer(&invite.code, &other.identity)
            .await?;
    }
    // All preflights were read-only: the first actual redeem still has its use.
    log.preview_invite(&invite.code).await?;
    log.redeem_agent_invite(&invite.code, &first.identity)
        .await?;
    assert!(
        log.preview_invite(&invite.code).await.is_err(),
        "spent links stay hidden publicly"
    );
    log.preview_agent_invite_for_redeemer(&invite.code, &first.identity)
        .await?;
    log.redeem_agent_invite(&invite.code, &first.identity)
        .await?;
    assert!(log
        .preview_agent_invite_for_redeemer(&invite.code, &other.identity)
        .await
        .is_err());
    assert!(log
        .redeem_agent_invite(&invite.code, &other.identity)
        .await
        .is_err());
    log.revoke_agent_invite(&agent).await?;
    assert!(
        log.preview_agent_invite_for_redeemer(&invite.code, &first.identity)
            .await
            .is_err(),
        "earlier redemption cannot reopen a revoked code"
    );
    log.drop_cell().await?;
    Ok(())
}
