//! The control producer retains its own exact publication, not arbitrary output.
use super::*;

type ProviderRows = Vec<(Vec<u8>, String, Vec<u8>, i64)>;
pub(super) fn provider_rows(engine: &Engine) -> rusqlite::Result<ProviderRows> {
    let mut query = engine.store.conn().prepare(
        "SELECT group_id,data_type,group_data,provider_version FROM openmls_group_data ORDER BY group_id,data_type",
    )?;
    let rows = query.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    rows.collect()
}

fn receipt() -> StdResult<(Fixture, OwnerPolicyScope, String, Envelope), Box<dyn std::error::Error>>
{
    let (mut f, scope, json) = setup()?;
    let status = submit(&mut f, &scope, &json)?;
    deliver_owner(&mut f)?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &status.source)?;
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    let envelope = f.core.engine.outbox_envelopes().remove(0);
    Ok((f, scope, status.source, envelope))
}

#[test]
fn control_original_and_exact_ciphertext_survive_cold_restore_before_accepted_echo() -> TestResult {
    let (mut f, scope, source, envelope) = receipt()?;
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    assert_eq!(f.core.engine.outbox_envelopes(), vec![envelope]);
    deliver_agent(&mut f)?;
    assert_eq!(
        f.owner.owner_policy_status(&scope, &source)?.state,
        "installed"
    );
    assert_eq!(f.core.engine.outbox_len(), 0);
    Ok(())
}

#[test]
fn control_echo_rejects_missing_or_changed_producer_proof_and_signed_header() -> TestResult {
    for case in [
        "reference",
        "source",
        "inner",
        "copy",
        "pending",
        "plaintext",
        "timestamp",
    ] {
        let (mut f, scope, _, envelope) = receipt()?;
        let image = f.core.image()?;
        let client_id = envelope.client_id().to_owned();
        let reference = format!("runtime.owner-policy.reference/1/{client_id}");
        let copy = format!("mls.sealed:{client_id}");
        let pending = f
            .core
            .engine
            .store
            .outbox_get(&client_id)?
            .ok_or("missing receipt")?;
        let mut echo = envelope.clone();
        match case {
            "reference" => f.core.engine.store.meta_delete(&reference)?,
            "source" => {
                let raw = f
                    .core
                    .engine
                    .store
                    .meta(&reference)?
                    .ok_or("missing reference")?;
                let mut value: serde_json::Value = serde_json::from_str(&raw)?;
                value["original_hash"] = "bb".repeat(32).into();
                f.core
                    .engine
                    .store
                    .set_meta(&reference, &serde_json::to_string(&value)?)?;
            }
            "inner" => {
                let actor = f.core.engine.net.author.clone().ok_or("missing agent")?;
                let event = actor.sign_event(
                    &pending.space,
                    &pending.client_id,
                    pending.at_ms,
                    pending.seen.clone(),
                    EventBody::OwnerPolicyReported {
                        source: "different_original".into(),
                        policy_digest: "cc".repeat(32),
                        outcome: types::OwnerPolicyOutcome::Installed,
                    },
                )?;
                f.core.engine.store.conn().execute(
                    "UPDATE outbox SET json=?1 WHERE client_id=?2",
                    rusqlite::params![serde_json::to_string(&event)?, client_id],
                )?;
            }
            "copy" => f.core.engine.store.meta_delete(&copy)?,
            "pending" => {
                f.core.engine.net.pending.remove(&client_id);
            }
            "plaintext" => echo = Envelope::plain(&pending),
            "timestamp" => {
                let actor = f.core.engine.net.author.clone().ok_or("missing agent")?;
                echo = Envelope::sealed(
                    &actor,
                    &pending.space,
                    &client_id,
                    pending.at_ms + 1,
                    pending.seen.as_ref(),
                    Sealed::new(
                        SealedKind::Application,
                        ::mls::SUITE_ID,
                        envelope
                            .sealed_data()
                            .ok_or("missing ciphertext")?
                            .1
                            .to_vec(),
                    ),
                );
            }
            _ => return Err("unknown case".into()),
        }
        if case != "pending" {
            f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
        }
        let head = f.core.engine.logs[&scope.space_id].head();
        let frame = ordered(&f.owner, echo);
        assert!(
            matches!(f.core.ingest(vec![frame]), Err(NativeError::History)),
            "{case}"
        );
        assert_eq!(f.core.engine.logs[&scope.space_id].head(), head, "{case}");
        assert_eq!(f.core.engine.store.outbox_len()?, 1, "{case}");
        assert!(f.core.image().is_err(), "{case}");
        let cold = DeviceCore::restore(image, f.credential())?;
        assert_eq!(cold.engine.outbox_envelopes(), vec![envelope], "{case}");
    }
    Ok(())
}

#[test]
fn failed_control_reference_write_rolls_back_outbox_ratchet_and_remains_retryable() -> TestResult {
    let (mut f, scope, json) = setup()?;
    let status = submit(&mut f, &scope, &json)?;
    deliver_owner(&mut f)?;
    let control = f
        .core
        .engine
        .owner_policy_submission(&scope.space_id, &status.source)?;
    let before = f.core.engine.logs[&scope.space_id].head();
    let ratchet = provider_rows(&f.core.engine)?;
    f.core.engine.store.conn().execute_batch("CREATE TEMP TRIGGER reject_policy_reference BEFORE INSERT ON meta WHEN NEW.key LIKE 'runtime.owner-policy.reference/%' BEGIN SELECT RAISE(ABORT,'injected reference failure'); END;")?;
    assert!(f
        .core
        .engine
        .report_owner_policy(&control, Some(control.digest()))
        .is_err());
    assert_eq!(f.core.engine.store.outbox_len()?, 0);
    assert_eq!(
        provider_rows(&f.core.engine)?,
        ratchet,
        "failed reference consumed no sender generation"
    );
    assert!(f.core.engine.store.conn().is_autocommit());
    assert!(f.core.engine.net.pending.is_empty());
    assert_eq!(f.core.engine.logs[&scope.space_id].head(), before);
    let references: i64 = f.core.engine.store.conn().query_row(
        "SELECT count(*) FROM meta WHERE key LIKE 'runtime.owner-policy.reference/%'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(references, 0);
    f.core
        .engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_policy_reference")?;
    f.core
        .engine
        .report_owner_policy(&control, Some(control.digest()))?;
    deliver_agent(&mut f)?;
    assert_eq!(
        f.owner.owner_policy_status(&scope, &status.source)?.state,
        "installed"
    );
    Ok(())
}

#[test]
fn explicit_options_refusal_is_a_retained_non_model_control_after_cold_restore() -> TestResult {
    let (mut f, request, hash, envelope) = super::super::image_review_cases::pending().unwrap();
    let frame = ordered(&f.owner, envelope);
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.ingest(vec![frame])?;
    f.owner.set_registered(true)?;
    f.core.engine.set_registered(true)?;
    let account = f.owner.account().ok_or("missing owner")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: f.space.clone(),
        agent_id: f.agent_profile.id.clone(),
    };
    let status = f
        .owner
        .request_owner_policy_options(&scope, &request, &hash)?;
    deliver_owner(&mut f)?;
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &status.source)?;
    f.core.engine.refuse_owner_policy_options(&query)?;
    f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
    deliver_agent(&mut f)?;
    let refused = f.owner.owner_policy_options(&scope, &status.source)?;
    assert_eq!(refused.state, "refused");
    assert!(refused.context.is_none());
    Ok(())
}

#[test]
fn accepted_control_replay_keeps_exact_original_after_outbox_removal_and_cold_restore() -> TestResult
{
    for case in ["reference", "opening", "position", "pruned", "timestamp"] {
        let (mut f, scope, _, envelope) = receipt()?;
        let original = ordered(&f.owner, envelope.clone());
        deliver_agent(&mut f)?;
        assert_eq!(f.core.engine.store.outbox_len()?, 0);
        f.core = DeviceCore::restore(f.core.image()?, f.credential())?;
        f.core.ingest(vec![original.clone()])?;
        let mut wrong = original.clone();
        match case {
            "reference" => f.core.engine.store.meta_delete(&format!(
                "runtime.owner-policy.reference/1/{}",
                envelope.client_id()
            ))?,
            "opening" => f
                .core
                .engine
                .store
                .meta_delete(&format!("runtime.mls-opening.v1/{}", original.hash))?,
            "position" => {
                wrong.seq += 1;
            }
            "pruned" => wrong.env = envelope.pruned().ok_or("cannot prune")?,
            "timestamp" => {
                let actor = f.core.engine.net.author.clone().ok_or("missing agent")?;
                wrong.env = Envelope::sealed(
                    &actor,
                    envelope.space(),
                    envelope.client_id(),
                    envelope.at_ms() + 1,
                    envelope.seen().as_ref(),
                    Sealed::new(
                        SealedKind::Application,
                        ::mls::SUITE_ID,
                        envelope
                            .sealed_data()
                            .ok_or("missing ciphertext")?
                            .1
                            .to_vec(),
                    ),
                );
            }
            _ => return Err("unknown replay case".into()),
        }
        let before = f.core.engine.logs[&scope.space_id].head();
        assert!(
            matches!(f.core.ingest(vec![wrong]), Err(NativeError::History)),
            "{case}"
        );
        assert_eq!(f.core.engine.logs[&scope.space_id].head(), before);
        assert_eq!(f.core.engine.store.outbox_len()?, 0);
    }
    Ok(())
}
