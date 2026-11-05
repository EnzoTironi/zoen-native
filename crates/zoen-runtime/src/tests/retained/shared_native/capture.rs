//! One actual loopback provider result survives failures without a second HTTP
//! request. Accounting is independent from current permission to act/publish.
use super::*;
use crate::tests::shared_policy_continuity::snapshot::{self, Snapshot};
pub(in crate::tests) async fn run() -> TestResult {
    for (fault, unknown, closed_sql, broken_fdb, broken_anchor, size) in [
        (
            119,
            false,
            false,
            false,
            false,
            crate::capture_codec::MAX_RESPONSE,
        ),
        (0, false, false, false, false, 0),
        (118, false, false, false, false, 0),
        (119, false, false, false, false, 0),
        (120, false, false, false, false, 0),
        (121, false, false, false, false, 0),
        (122, false, false, false, false, 0),
        (118, true, false, false, false, 0),
        (0, false, true, false, false, 0),
        (0, false, false, true, false, 0),
        (0, false, false, false, true, 0),
        (0, false, false, false, false, 300_000),
        (118, false, false, false, false, 300_000),
        (119, false, false, false, false, 300_000),
        (122, false, false, false, false, 300_000),
        (0, true, false, false, false, 300_000),
        (0, false, false, false, true, 300_000),
    ] {
        let mut f = if size == 0 {
            Fixture::with_shared_runtime().await?
        } else {
            Fixture::with_shared_response(crate::capture_codec::MAX_RESPONSE).await?
        };
        if size > 0 {
            let mut body = f
                .world
                .http
                .body
                .lock()
                .map_err(|_| "HTTP fixture poisoned")?;
            body["choices"][0]["message"]["content"] = serde_json::Value::String(String::new());
            let overhead = serde_json::to_vec(&*body)?.len();
            let content = if size == crate::capture_codec::MAX_RESPONSE {
                size.checked_sub(overhead).ok_or("body overhead")?
            } else {
                size
            };
            body["choices"][0]["message"]["content"] =
                serde_json::Value::String("x".repeat(content));
        }
        f.grant("shared-capture-owner").await?;
        f.message(ORIGINAL).await?;
        if unknown {
            f.world
                .http
                .body
                .lock()
                .map_err(|_| "HTTP fixture poisoned")?
                .as_object_mut()
                .ok_or("fixture object")?
                .remove("usage");
        }
        let report = f.sync(false).await?;
        let run = report.runs.first().ok_or("original run")?.clone();
        let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
        let claim = native::testing::claim_retained_reply(&f.world.runtime, &run.run)
            .await?
            .ok_or("live original claim")?;
        if size == crate::capture_codec::MAX_RESPONSE {
            let storage: String = sqlx::query_scalar(
                "SELECT attstorage::text FROM pg_attribute WHERE attrelid='runtime_evidence'::regclass AND attname='sealed'",
            ).fetch_one(&f.world.runtime.finance.pool).await?;
            assert_eq!(storage, "e");
        }
        if size > 0 {
            // A large declared profile alone is not the original send witness.
            let mut sql = f.world.runtime.finance.pool.begin().await?;
            let premature =
                sqlx::query("INSERT INTO runtime_evidence(attempt,digest,sealed) VALUES($1,$2,$3)")
                    .bind(&run.attempt)
                    .bind("d".repeat(64))
                    .bind(vec![0_u8; 98_305])
                    .execute(&mut *sql)
                    .await;
            sql.rollback().await?;
            assert!(
                premature
                    .as_ref()
                    .err()
                    .and_then(sqlx::Error::as_database_error)
                    .and_then(|error| error.code())
                    .is_some_and(|code| code == "23514"),
                "SQL requires the original admission witness before accepting large chat evidence"
            );
        }
        let capture =
            native::testing::complete_retained_for_capture(&f.world.runtime, &run.run, claim)
                .await?;
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 1);
        let (digest, sealed) = crate::capture_codec::seal(&f.world.runtime.custody, &capture)?;
        if size > 0 {
            assert!(sealed.len() > crate::custody::MAX_SEALED);
        }
        if size == crate::capture_codec::MAX_RESPONSE {
            assert_eq!(capture.result.evidence.body.len(), size);
            assert!(
                sealed.len() > 32 * 1024 * 1024,
                "full gateway output crosses the previous generic payload bound"
            );
        }
        if size == 0 && fault == 0 && !closed_sql && !broken_fdb && !broken_anchor {
            let mut sql = f.world.runtime.finance.pool.begin().await?;
            let oversized =
                sqlx::query("INSERT INTO runtime_evidence(attempt,digest,sealed) VALUES($1,$2,$3)")
                    .bind(&run.attempt)
                    .bind("d".repeat(64))
                    .bind(vec![0_u8; 98_305])
                    .execute(&mut *sql)
                    .await;
            sql.rollback().await?;
            assert!(
                oversized
                    .as_ref()
                    .err()
                    .and_then(sqlx::Error::as_database_error)
                    .and_then(|error| error.code())
                    .is_some_and(|code| code == "23514"),
                "old frozen chat profiles retain their original SQL size boundary"
            );
        }
        let backup = if fault == 0 && !closed_sql && !broken_fdb && !broken_anchor {
            Some(Snapshot::capture(&f.world)?)
        } else {
            None
        };
        let before = checkpoint(&f).await?;
        // Revocation after a known admitted expense cannot erase that expense.
        sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
            .bind(&binding.context.device)
            .execute(&f.world.runtime.finance.pool)
            .await?;
        let deployment_key = f.world.runtime.execution.root.pack(&("deployment",));
        let original_deployment = if broken_fdb {
            let tx = f.world.runtime.execution.transaction().await?;
            let original = tx
                .get(&deployment_key, false)
                .await?
                .ok_or("original deployment")?
                .to_vec();
            tx.set(&deployment_key, b"unavailable-original-pair");
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            Some(original)
        } else {
            None
        };
        let anchor_originals = f
            .shared_directory
            .as_ref()
            .ok_or("shared directory")?
            .0
            .join("originals");
        if broken_anchor {
            std::fs::write(&anchor_originals, b"fixture unavailable original directory")?;
        }
        if closed_sql {
            f.world.runtime.finance.pool.close().await;
        }
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let mut result = f
            .world
            .runtime
            .capture_shared(&run.attempt, &digest, &sealed)
            .await;
        if size == crate::capture_codec::MAX_RESPONSE
            && result == Err(RuntimeError::Unavailable)
            && f.world.count("SELECT count(*) FROM runtime_evidence").await == 0
        {
            let no_prepared = f
                .world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("managed")?
                .lock()
                .await
                .anchor
                .pending()?
                .is_none();
            if no_prepared {
                // A real storage timeout before PREPARED is not the requested
                // SQL-ACK cut. Retry only this exact retained result once;
                // never repeat provider execution or manufacture admission.
                eprintln!("maximum capture: retry exact original before PREPARED");
                result = f
                    .world
                    .runtime
                    .capture_shared(&run.attempt, &digest, &sealed)
                    .await;
            }
        }
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        let expected = if unknown {
            FinancialState::Claimed
        } else {
            FinancialState::Settled { units: 6 }
        };
        if fault == 0 && !closed_sql && !broken_fdb && !broken_anchor {
            assert_eq!(result?, expected);
        } else {
            assert!(result.is_err());
        }
        if fault == 119 && size > 0 {
            assert_eq!(
                f.world.count("SELECT count(*) FROM runtime_evidence").await,
                1,
                "the injected lost acknowledgement follows the actual SQL evidence commit"
            );
        }
        // The independent source is durable even before either bank is touched.
        {
            let managed = f
                .world
                .runtime
                .runtime_continuity
                .as_ref()
                .ok_or("managed")?
                .lock()
                .await;
            if broken_anchor {
                assert!(managed.anchor.reader().retained_originals().is_err());
            } else {
                assert_eq!(managed.anchor.reader().retained_originals()?.len(), 1);
            }
        }
        if broken_anchor {
            assert_eq!(
                f.world.runtime.execution.pending_evidence(8).await?.len(),
                1
            );
            std::fs::remove_file(&anchor_originals)?;
        }
        if let Some(original) = original_deployment {
            let tx = f.world.runtime.execution.db.create_trx()?;
            tx.set_option(foundationdb::options::TransactionOption::Timeout(5000))?;
            tx.set(&deployment_key, &original);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        }
        // A PREPARED operation may recover missing physical chunks from its
        // independent original. A clean committed operation must still refuse
        // an undeclared missing chunk below, without silently repairing it.
        let chunk_key = f.world.runtime.execution.root.pack(&(
            "evidence-chunk/1",
            run.attempt.as_str(),
            digest.as_str(),
            crate::hash(&sealed).as_str(),
            0_i64,
        ));
        if size > 0 && fault == 118 {
            let tx = f.world.runtime.execution.transaction().await?;
            assert!(tx.get(&chunk_key, false).await?.is_some());
            tx.clear(&chunk_key);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        }
        reopen(&mut f).await?;
        f.world.runtime.reconcile(8).await?;
        let cp = checkpoint(&f).await?;
        assert_eq!(cp.sequence, before.sequence + 1);
        assert_eq!(
            f.world.runtime.inspect_attempt(&run.attempt).await?,
            Some(expected.clone())
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_evidence").await,
            1
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_claims").await,
            1
        );
        assert_eq!(
            f.world
                .count("SELECT count(*) FROM runtime_admission_witnesses")
                .await,
            1
        );
        assert_eq!(
            f.world.balance().await.spent_units,
            if unknown { 0 } else { 6 }
        );
        assert_eq!(
            f.world.balance().await.held_units,
            if unknown { binding.hold_units } else { 0 }
        );
        let retained: Vec<u8> = sqlx::query_scalar(
            "SELECT sealed FROM runtime_evidence WHERE attempt=$1 AND digest=$2",
        )
        .bind(&run.attempt)
        .bind(&digest)
        .fetch_one(&f.world.runtime.finance.pool)
        .await?;
        assert!(
            retained.iter().eq(sealed.iter()),
            "SQL retains the original ciphertext"
        );
        let evidence_key = f.world.runtime.execution.root.pack(&(
            "evidence",
            run.attempt.as_str(),
            digest.as_str(),
        ));
        let tx = f.world.runtime.execution.transaction().await?;
        let original = tx
            .get(&evidence_key, false)
            .await?
            .ok_or("retained FDB result")?
            .to_vec();
        let payload = f
            .world
            .runtime
            .execution
            .read_evidence_payload_in(&tx, &run.attempt, &digest, &original)
            .await?;
        assert!(
            payload.iter().eq(sealed.iter()),
            "FDB retains the original ciphertext"
        );
        if size > 0 {
            assert!(
                original.len() < sealed.len(),
                "FDB root is a small manifest"
            );
        }
        tx.clear(&evidence_key);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        assert_eq!(
            f.world.runtime.check_shared_runtime().await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        let tx = f.world.runtime.execution.transaction().await?;
        tx.set(&evidence_key, &original);
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        f.world.runtime.check_shared_runtime().await?;
        assert_eq!(
            f.world
                .runtime
                .capture_shared(&run.attempt, &digest, &sealed)
                .await?,
            expected
        );
        assert_eq!(
            checkpoint(&f).await?,
            cp,
            "exact replay does not add a checkpoint or charge"
        );
        if size > 0 {
            let tx = f.world.runtime.execution.transaction().await?;
            let chunk = tx
                .get(&chunk_key, false)
                .await?
                .ok_or("original physical chunk")?
                .to_vec();
            tx.clear(&chunk_key);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            assert!(f.world.runtime.check_shared_runtime().await.is_err());
            assert!(f
                .world
                .runtime
                .capture_shared(&run.attempt, &digest, &sealed)
                .await
                .is_err());
            let tx = f.world.runtime.execution.transaction().await?;
            assert!(
                tx.get(&chunk_key, false).await?.is_none(),
                "clean replay cannot manufacture a missing committed chunk"
            );
            tx.set(&chunk_key, &chunk);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            f.world.runtime.check_shared_runtime().await?;
        }
        let mut changed = sealed.clone();
        let byte = changed.last_mut().ok_or("nonempty original")?;
        *byte ^= 1;
        assert!(f
            .world
            .runtime
            .capture_shared(&run.attempt, &digest, &changed)
            .await
            .is_err());
        assert_eq!(checkpoint(&f).await?, cp);
        assert!(
            f.world
                .runtime
                .finance
                .capture(&capture, &digest, &sealed)
                .await
                .is_err(),
            "unmanaged financial mutation cannot bypass shared continuity"
        );
        if let Some(backup) = backup {
            Arc::get_mut(&mut f.world.runtime)
                .ok_or("exclusive runtime")?
                .runtime_continuity = None;
            snapshot::restored_finance_refuses(
                &f.world,
                &f.shared_directory.as_ref().ok_or("shared directory")?.0,
                &backup,
                &cp,
                1,
                1,
                1,
            )
            .await?;
            reopen(&mut f).await?;
        }
        reopen(&mut f).await?;
        f.world.runtime.reconcile(8).await?;
        assert_eq!(checkpoint(&f).await?, cp);
        assert_eq!(
            f.world.balance().await.spent_units,
            if unknown { 0 } else { 6 }
        );
        assert_eq!(
            f.world.http.sends.load(Ordering::SeqCst),
            1,
            "recovery never calls the provider again"
        );
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        f.finish().await;
        println!("shared capture fault={fault} unknown={unknown} closed_sql={closed_sql} broken_fdb={broken_fdb} broken_anchor={broken_anchor} size={size}: one actual HTTP/admission, independent encrypted original, late revoked accounting, exact cold recovery/no double charge, retained unknown cost, original SQL/FDB checks and old SQL backup refusal PASS");
    }
    Ok(())
}
