//! Real signed clients, canonical head, recipient, refusal and receipt recovery.
use super::app_edit_request_cases::{add_agent, input, with_agent};
use super::*;
use crate::app_edit_requests::{AppEditRequestInput, AppEditTargetInput};
type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) fn confirm(f: &Fixture) -> TestResult {
    let pending = f.client.lock().store.outbox()?;
    for row in pending.into_iter().filter(|r| !r.failed) {
        let mut engine = f.client.lock();
        let mut log = engine
            .logs
            .get(&row.event.space)
            .cloned()
            .unwrap_or_else(|| event_log::SpaceLog::new(&row.event.space));
        let event = log.sequence(row.event)?;
        let outcome = engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event)?,
        });
        assert!(matches!(
            outcome,
            crate::Ingest::Confirmed | crate::Ingest::Applied
        ));
    }
    Ok(())
}
fn select(
    f: &Fixture,
    base: &AppBinaryDocumentDto,
    id: &str,
    agent: &str,
) -> R<AppEditRequestInput> {
    confirm(f).map_err(|e| crate::CoreError::Storage {
        message: e.to_string(),
    })?;
    let mut command = input(f, base, id)?;
    let targets = f.client.lock().app_edit_targets(&(&command).into())?;
    command.target_agent_id = Some(agent.into());
    command.expected_target_revision = Some(targets.revision);
    Ok(command)
}
fn bound_agent(f: &Fixture, entry: &str) -> R<Option<String>> {
    Ok(f.client
        .lock()
        .artifact_request(&f.space, entry)?
        .and_then(|r| r.agent_id))
}
#[test]
fn multiple_owned_writers_require_selection_and_bind_the_exact_agent_in_signed_source() -> TestResult
{
    let (f, first) = with_agent(":memory:")?;
    let second = add_agent(&f, &f.client, [86; 32], Role::Member)?;
    f.client
        .lock()
        .set_trust(&second, &f.space, types::TrustLevel::Suggest)?;
    let base = create(&f, "target_base");
    let mut legacy = input(&f, &base, "ambiguous")?;
    legacy.share_current = true;
    let before = counts(&f);
    assert!(f.client.lock().request_app_edit(legacy).is_err());
    assert_eq!(counts(&f), before, "no share, source, document or grant");
    let command = select(&f, &base, "selected", &second)?;
    let posted = f.client.lock().request_app_edit(command.clone())?;
    assert_eq!(bound_agent(&f, &posted.id)?, Some(second));
    assert_ne!(bound_agent(&f, &posted.id)?, Some(first));
    let after = counts(&f);
    assert_eq!(f.client.lock().request_app_edit(command)?.id, posted.id);
    assert_eq!(counts(&f), after);
    Ok(())
}
#[test]
fn target_context_is_read_only_stable_and_not_a_membership_or_trust_grant() -> TestResult {
    let (f, agent) = with_agent(":memory:")?;
    let base = create(&f, "targets");
    let i: AppEditTargetInput = (&input(&f, &base, "read")?).into();
    confirm(&f)?;
    let before = counts(&f);
    let a = f.client.lock().app_edit_targets(&i)?;
    let b = f.client.lock().app_edit_targets(&i)?;
    assert_eq!(a.revision, b.revision);
    assert_eq!(
        a.agents
            .iter()
            .map(|a| a.agent.id.as_str())
            .collect::<Vec<_>>(),
        vec![agent.as_str()]
    );
    assert_eq!(counts(&f), before);
    let command = input(&f, &base, "single_legacy")?;
    let posted = f.client.lock().request_app_edit(command)?;
    assert_eq!(bound_agent(&f, &posted.id)?, Some(agent));
    Ok(())
}
#[test]
fn foreign_removed_reader_and_unknown_targets_cannot_publish_even_with_a_current_revision(
) -> TestResult {
    let (f, eligible) = with_agent(":memory:")?;
    let foreign = fixture(":memory:");
    let other = add_agent(&f, &foreign.client, [87; 32], Role::Member)?;
    let reader = add_agent(&f, &f.client, [88; 32], Role::Reader)?;
    let removed = add_agent(&f, &f.client, [89; 32], Role::Member)?;
    f.client.remove_member(f.space.clone(), removed.clone())?;
    let base = create(&f, "refusal_base");
    for target in [&other, &reader, &removed, "missing"] {
        let mut command = select(&f, &base, "refused", &eligible)?;
        command.target_agent_id = Some(target.into());
        command.share_current = true;
        let before = counts(&f);
        assert!(
            f.client.lock().request_app_edit(command).is_err(),
            "{target}"
        );
        assert_eq!(counts(&f), before);
    }
    Ok(())
}
#[test]
fn changed_trust_or_membership_and_aba_selection_require_a_new_observation() -> TestResult {
    for change in ["listen", "removed", "regrant", "aba"] {
        let (f, agent) = with_agent(":memory:")?;
        f.client
            .lock()
            .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
        let base = create(&f, "stale_selection");
        let mut command = select(&f, &base, "stale", &agent)?;
        command.share_current = true;
        {
            let mut e = f.client.lock();
            match change {
                "listen" => e.set_trust(&agent, &f.space, types::TrustLevel::Listen)?,
                "regrant" => e.set_trust(&agent, &f.space, types::TrustLevel::Suggest)?,
                _ => {
                    e.remove_member(&f.space, &agent)?;
                    if change == "aba" {
                        e.add_member(&f.space, &agent, Role::Member)?;
                    }
                }
            }
        }
        let before = counts(&f);
        assert!(
            f.client.lock().request_app_edit(command).is_err(),
            "{change}"
        );
        assert_eq!(counts(&f), before);
    }
    Ok(())
}
#[test]
fn signed_expired_or_revoked_latest_grant_never_revives_implicit_trust() -> TestResult {
    for expired in [true, false] {
        let (f, agent) = with_agent(":memory:")?;
        f.client
            .lock()
            .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
        let base = create(&f, "trust_base");
        let mut command = select(&f, &base, "trust_refused", &agent)?;
        command.share_current = true;
        {
            let mut e = f.client.lock();
            let owner = e.me_id()?;
            let grant = types::Grant {
                id: types::new_id("gr")?,
                grantor: owner.clone(),
                grantee: Some(agent),
                scope: types::GrantScope::Space(f.space.clone()),
                capability: types::Capability::Trust(types::TrustLevel::Suggest),
                expires_at_ms: expired.then_some(crate::engine::now_ms() - 1),
            };
            e.append(
                &f.space,
                &owner,
                types::EventBody::GrantIssued {
                    grant: grant.clone(),
                },
            )?;
            if !expired {
                e.append(
                    &f.space,
                    &owner,
                    types::EventBody::GrantRevoked { grant: grant.id },
                )?;
            }
        }
        let before = counts(&f);
        assert!(f.client.lock().request_app_edit(command).is_err());
        assert_eq!(counts(&f), before);
        let read: AppEditTargetInput = (&input(&f, &base, "read")?).into();
        assert!(f
            .client
            .lock()
            .app_edit_targets(&read)?
            .agents
            .iter()
            .all(|a| !a.eligible && a.permission_required));
    }
    Ok(())
}
#[test]
fn wrong_owner_head_pin_or_missing_selection_pair_is_inert() -> TestResult {
    let (f, agent) = with_agent(":memory:")?;
    f.client
        .lock()
        .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
    let base = create(&f, "context_base");
    for change in 0..7 {
        let mut command = select(&f, &base, "bad_context", &agent)?;
        command.share_current = true;
        match change {
            0 => command.expected_device_id = "other".into(),
            1 => command.expected_identity_id = agent.clone(),
            2 => command.expected_version += 1,
            3 => command.expected_sha256 = "ff".repeat(32),
            4 => command.expected_resource_uri = Some("ui://forged".into()),
            5 => command.expected_target_revision = None,
            _ => command.target_agent_id = None,
        }
        let before = counts(&f);
        assert!(f.client.lock().request_app_edit(command).is_err());
        assert_eq!(counts(&f), before);
    }
    Ok(())
}
#[test]
fn selected_receipt_survives_lost_ack_cold_unlock_and_later_target_revocation() -> TestResult {
    let directory = std::env::temp_dir().join(types::new_id("target_receipt")?);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("owner.sqlite");
    let (f, agent) = with_agent(path.to_str().ok_or("fixture path")?)?;
    let second = add_agent(&f, &f.client, [90; 32], Role::Member)?;
    f.client
        .lock()
        .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
    let base = create(&f, "cold_base");
    let command = select(&f, &base, "lost_ack", &agent)?;
    let posted = f.client.lock().request_app_edit(command.clone())?;
    f.client.remove_member(f.space.clone(), agent.clone())?;
    for change in ["agent", "revision", "share", "text"] {
        let mut redirected = command.clone();
        match change {
            "agent" => redirected.target_agent_id = Some(second.clone()),
            "revision" => redirected.expected_target_revision = Some("ff".repeat(32)),
            "share" => redirected.share_current = true,
            _ => redirected.text = "Different intent".into(),
        }
        let before = counts(&f);
        assert!(
            f.client.lock().request_app_edit(redirected).is_err(),
            "{change}"
        );
        assert_eq!(counts(&f), before);
    }
    let vault = f.vault.clone();
    let space = f.space.clone();
    drop(f);
    let reopened = Client::open(path.to_str().ok_or("fixture path")?.into(), "en".into())?;
    assert!(reopened.unlock(vault)?);
    let before = reopened.lock().outbox_len();
    assert_eq!(reopened.lock().request_app_edit(command)?.id, posted.id);
    assert_eq!(reopened.lock().outbox_len(), before);
    assert_eq!(
        reopened
            .lock()
            .artifact_request(&space, &posted.id)?
            .and_then(|r| r.agent_id),
        Some(agent)
    );
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn ordinary_chat_and_its_ack_do_not_retire_an_unchanged_target_selection() -> TestResult {
    let (f, agent) = with_agent(":memory:")?;
    f.client
        .lock()
        .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
    let base = create(&f, "chat_stable_base");
    let command = select(&f, &base, "stable_target", &agent)?;
    let read: AppEditTargetInput = (&command).into();
    let prior = f.client.lock().app_edit_targets(&read)?.revision;
    f.client
        .send_message(f.space.clone(), "Unrelated chat progress".into())?;
    assert_eq!(f.client.lock().app_edit_targets(&read)?.revision, prior);
    confirm(&f)?;
    assert_eq!(f.client.lock().app_edit_targets(&read)?.revision, prior);
    f.client.lock().request_app_edit(command)?;
    Ok(())
}
