//! Actual retained result -> original financial ACK -> cold recovery, no new
//! execution, publication, refund or ledger. No second HTTP call is permitted.
use super::*;
use crate::tests::shared_policy_continuity::snapshot::{self, Snapshot};

async fn subjects(f: &Fixture) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(f.world
        .runtime
        .runtime_continuity
        .as_ref()
        .ok_or("managed")?
        .lock()
        .await
        .anchor
        .reader()
        .subject_commitments()?
        .len())
}
pub(super) async fn captured(
    f: &mut Fixture,
    unknown: bool,
    maximum: bool,
) -> Result<(String, String, Vec<u8>, i64), Box<dyn std::error::Error>> {
    if maximum {
        let mut body = f.world.http.body.lock().map_err(|_| "HTTP body")?;
        body["choices"][0]["message"]["content"] = serde_json::Value::String(String::new());
        let overhead = serde_json::to_vec(&*body)?.len();
        body["choices"][0]["message"]["content"] = serde_json::Value::String(
            "x".repeat(
                crate::capture_codec::MAX_RESPONSE
                    .checked_sub(overhead)
                    .ok_or("HTTP overhead")?,
            ),
        );
    }
    if unknown {
        f.world
            .http
            .body
            .lock()
            .map_err(|_| "HTTP body")?
            .as_object_mut()
            .ok_or("HTTP object")?
            .remove("usage");
    }
    f.grant("shared-ack-owner").await?;
    f.message(ORIGINAL).await?;
    let run = f.sync(false).await?.runs.into_iter().next().ok_or("run")?;
    let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
    let claim = native::testing::claim_retained_reply(&f.world.runtime, &run.run)
        .await?
        .ok_or("claim")?;
    let capture =
        native::testing::complete_retained_for_capture(&f.world.runtime, &run.run, claim).await?;
    let (digest, sealed) = crate::capture_codec::seal(&f.world.runtime.custody, &capture)?;
    eprintln!("shared ACK phase=before-capture");
    let before = checkpoint(f).await?;
    assert!(
        f.world
            .runtime
            .ack_shared_outbox(&run.attempt, "evidence", &digest)
            .await
            .is_err(),
        "no ACK before actual captured evidence"
    );
    assert_eq!(checkpoint(f).await?, before);
    eprintln!("shared ACK phase=retain-capture");
    match f
        .world
        .runtime
        .capture_shared(&run.attempt, &digest, &sealed)
        .await
    {
        Ok(_) => {}
        Err(RuntimeError::Unavailable) if maximum => {
            f.world.runtime.reconcile_shared_captures(8).await?;
        }
        Err(error) => return Err(error.into()),
    }
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_evidence").await,
        1
    );
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(&binding.context.device)
        .execute(&f.world.runtime.finance.pool)
        .await?;
    Ok((run.attempt, digest, sealed, binding.hold_units))
}
async fn chunk_boundaries(f: &Fixture, attempt: &str, digest: &str, sealed: &[u8]) -> TestResult {
    let cipher = crate::hash(sealed);
    let last = sealed
        .len()
        .div_ceil(64 * 1024)
        .checked_sub(1)
        .ok_or("chunks")?;
    for index in [16_usize, last] {
        let key = f.world.runtime.execution.root.pack(&(
            "evidence-chunk/1",
            attempt,
            digest,
            cipher.as_str(),
            i64::try_from(index)?,
        ));
        let tx = f.world.runtime.execution.transaction().await?;
        let original = tx.get(&key, false).await?.ok_or("actual chunk")?.to_vec();
        if index == 16 {
            let mut changed = original.clone();
            let byte = changed.first_mut().ok_or("chunk bytes")?;
            *byte ^= 1;
            tx.set(&key, &changed);
        } else {
            tx.clear(&key);
        }
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(
            f.world.runtime.check_shared_runtime().await.is_err(),
            "batched reads must verify middle and final physical chunks"
        );
        let tx = f.world.runtime.execution.transaction().await?;
        if index == last {
            assert!(
                tx.get(&key, false).await?.is_none(),
                "no repair of clean missing data"
            );
        }
        tx.set(&key, &original);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    }
    f.world.runtime.check_shared_runtime().await?;
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    for (fault, unknown, maximum, kind) in [
        (131, false, true, "settle"),
        (0, false, false, "evidence"),
        (130, false, false, "evidence"),
        (131, false, false, "evidence"),
        (132, false, false, "evidence"),
        (133, false, false, "evidence"),
        (130, true, false, "evidence"),
        (131, false, false, "settle"),
    ] {
        let mut f = if maximum {
            Fixture::with_shared_response(crate::capture_codec::MAX_RESPONSE).await?
        } else {
            Fixture::with_shared_runtime().await?
        };
        eprintln!("shared ACK case fault={fault} unknown={unknown} maximum={maximum} kind={kind}");
        let (attempt, digest, sealed, hold) = captured(&mut f, unknown, maximum).await?;
        if maximum {
            chunk_boundaries(&f, &attempt, &digest, &sealed).await?;
        }
        let count = subjects(&f).await?;
        let journals = f.world.count("SELECT count(*) FROM runtime_journals").await;
        let postings = f.world.count("SELECT count(*) FROM runtime_postings").await;
        let backup = if !maximum && fault == 131 {
            Some(Snapshot::capture(&f.world)?)
        } else {
            None
        };
        if kind == "settle" {
            f.world
                .runtime
                .ack_shared_outbox(&attempt, "evidence", &digest)
                .await?;
        }
        let before = checkpoint(&f).await?;
        assert!(
            f.world
                .runtime
                .finance
                .ack_outbox(&attempt, kind, &digest)
                .await
                .is_err(),
            "unmanaged ACK is not a shared shortcut"
        );
        assert!(f
            .world
            .runtime
            .ack_shared_outbox(&attempt, "release", &digest)
            .await
            .is_err());
        assert!(f
            .world
            .runtime
            .ack_shared_outbox(&attempt, kind, &"f".repeat(64))
            .await
            .is_err());
        if unknown {
            assert!(f
                .world
                .runtime
                .ack_shared_outbox(&attempt, "settle", &digest)
                .await
                .is_err());
        }
        assert_eq!(checkpoint(&f).await?, before);
        eprintln!("shared ACK phase=commit");
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let ack = f
            .world
            .runtime
            .ack_shared_outbox(&attempt, kind, &digest)
            .await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            ack?;
        } else {
            assert_eq!(ack, Err(RuntimeError::Unavailable));
        }
        if fault == 130 || fault == 131 || fault == 132 {
            let managed = f
                .world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("managed")?
                .lock()
                .await;
            let pending = managed.anchor.pending()?.ok_or("original pending ACK")?;
            f.world.runtime.test_ack_intent(&pending)?;
            let mut tampered = pending.clone();
            tampered.effect_digest = "f".repeat(64);
            assert!(f.world.runtime.test_ack_intent(&tampered).is_err());
            let mut tampered = pending.clone();
            tampered.subject_updates[0].next = "f".repeat(64);
            assert!(f.world.runtime.test_ack_intent(&tampered).is_err());
            let mut tampered = pending;
            tampered.retirement = true;
            assert!(f.world.runtime.test_ack_intent(&tampered).is_err());
        }
        if fault == 131 && kind == "evidence" {
            if let Some(backup) = backup.as_ref() {
                let next = f
                    .world
                    .runtime
                    .runtime_continuity
                    .as_ref()
                    .ok_or("managed")?
                    .lock()
                    .await
                    .anchor
                    .pending()?
                    .ok_or("pending original ACK")?
                    .next;
                Arc::get_mut(&mut f.world.runtime)
                    .ok_or("exclusive runtime")?
                    .runtime_continuity = None;
                // The real old backup has the captured result/outbox but lacks
                // this ACK. Overlaying next markers must not recreate it.
                snapshot::restored_finance_refuses(
                    &f.world,
                    &f.shared_directory.as_ref().ok_or("shared directory")?.0,
                    backup,
                    &next,
                    1,
                    1,
                    1,
                )
                .await?;
            }
        }
        eprintln!("shared ACK phase=cold-recovery");
        reopen(&mut f).await?;
        assert_eq!(checkpoint(&f).await?.sequence, before.sequence + 1);
        assert_eq!(
            subjects(&f).await?,
            count,
            "ACK refines the existing capture, not another subject"
        );
        let expected = if unknown {
            FinancialState::Claimed
        } else {
            FinancialState::Settled { units: 6 }
        };
        assert_eq!(
            f.world.runtime.inspect_attempt(&attempt).await?,
            Some(expected)
        );
        let key = f.world.runtime.execution.root.pack(&(
            "financial",
            attempt.as_str(),
            kind,
            digest.as_str(),
        ));
        let tx = f.world.runtime.execution.transaction().await?;
        assert_eq!(tx.get(&key, false).await?.as_deref(), Some(b"1".as_slice()));
        tx.clear(&key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert!(
            f.world.runtime.reconcile(8).await.is_err(),
            "a clean missing actual receipt is not silently repaired"
        );
        let tx = f.world.runtime.execution.transaction().await?;
        assert!(tx.get(&key, false).await?.is_none());
        tx.set(&key, b"1");
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        eprintln!("shared ACK phase=public-drain");
        // Public reconciliation drains every remaining incurred-result ACK.
        f.world.runtime.reconcile(8).await?;
        assert!(f
            .world
            .runtime
            .finance
            .pending_outbox_keys(8)
            .await?
            .is_empty());
        assert_eq!(
            f.world
                .count("SELECT count(*) FROM runtime_financial_acks")
                .await,
            if unknown { 1 } else { 2 }
        );
        assert_eq!(subjects(&f).await?, count);
        let tip = checkpoint(&f).await?;
        f.world
            .runtime
            .ack_shared_outbox(&attempt, kind, &digest)
            .await?;
        assert_eq!(f.world.runtime.reconcile(8).await?.inspected, 0);
        assert_eq!(checkpoint(&f).await?, tip);
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_journals").await,
            journals
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_postings").await,
            postings
        );
        assert_eq!(
            f.world.balance().await.spent_units,
            if unknown { 0 } else { 6 }
        );
        assert_eq!(
            f.world.balance().await.held_units,
            if unknown { hold } else { 0 }
        );
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        // Retrying the exact captured result after receipts preserves the tip.
        f.world
            .runtime
            .capture_shared(&attempt, &digest, &sealed)
            .await?;
        assert_eq!(checkpoint(&f).await?, tip);
        if let Some(backup) = backup {
            Arc::get_mut(&mut f.world.runtime)
                .ok_or("exclusive runtime")?
                .runtime_continuity = None;
            snapshot::restored_finance_refuses(
                &f.world,
                &f.shared_directory.as_ref().ok_or("shared directory")?.0,
                &backup,
                &tip,
                1,
                1,
                1,
            )
            .await?;
        }
        reopen(&mut f).await?;
        assert_eq!(f.world.runtime.reconcile(8).await?.inspected, 0);
        assert_eq!(checkpoint(&f).await?, tip);
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        f.finish().await;
        println!("shared incurred ACK fault={fault} unknown={unknown} maximum={maximum} kind={kind}: original receipts, cold cuts, no duplicate bill/provider, held unknown cost, clean missing receipt refusal and public drain PASS");
    }
    Ok(())
}
