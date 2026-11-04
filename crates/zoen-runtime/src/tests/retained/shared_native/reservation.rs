//! Real native reservations preserve the original period/hold across crashes.
use super::*;
use crate::tests::shared_policy_continuity::snapshot::{self, Snapshot};
pub(super) async fn run() -> TestResult {
    for fault in [0, 102, 103, 104, 105] {
        let mut f = Fixture::with_shared_runtime().await?;
        f.grant("shared-reserve-owner").await?;
        f.message(ORIGINAL).await?;
        let sync = f.sync(false).await?;
        let run = sync.runs.first().ok_or("discovered original")?.clone();
        let backup = if fault == 0 {
            Some(Snapshot::capture(&f.world)?)
        } else {
            None
        };
        let before = checkpoint(&f).await?;
        f.world.runtime.fault.store(fault, Ordering::SeqCst);
        let result = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await;
        f.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            result?;
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
        }
        reopen(&mut f).await?;
        let cp = checkpoint(&f).await?;
        assert_eq!(cp.sequence, before.sequence + 1);
        let binding = native::testing::reserve_retained_reply(&f.world.runtime, &run.run).await?;
        assert_eq!(checkpoint(&f).await?, cp);
        assert!(binding.hold_units > 0);
        assert_eq!(f.world.balance().await.held_units, binding.hold_units);
        assert_eq!(f.world.balance().await.spent_units, 0);
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_attempts").await,
            1
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_journals").await,
            1
        );
        assert_eq!(
            f.world.count("SELECT count(*) FROM runtime_postings").await,
            2
        );
        assert_eq!(
            f.world.runtime.inspect_attempt(&run.attempt).await?,
            Some(FinancialState::Reserved)
        );
        assert_eq!(
            f.world.runtime.finance.reserve(&binding).await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(matches!(
            f.world.runtime.finance.claim(&binding).await,
            Err(RuntimeError::RestoreUnreconciled)
        ));
        assert_eq!(checkpoint(&f).await?, cp);
        if let Some(backup) = backup {
            Arc::get_mut(&mut f.world.runtime)
                .ok_or("exclusive current runtime")?
                .runtime_continuity = None;
            snapshot::restored_finance_refuses(
                &f.world,
                &f.shared_directory.as_ref().ok_or("shared directory")?.0,
                &backup,
                &cp,
            )
            .await?;
            reopen(&mut f).await?;
            assert_eq!(f.world.balance().await.held_units, binding.hold_units);
            assert_eq!(checkpoint(&f).await?, cp);
            // A real second run sees the existing owner-wide hold under the
            // actual signed tighter policy; there is no separate agent balance.
            let mut policy = f.world.signed.policy.clone();
            policy.version += 1;
            policy.previous_digest = Some(f.world.policy_digest.clone());
            policy.limit_units = binding.hold_units;
            policy.max_attempt_units = binding.hold_units;
            let signed =
                event_log::owner_budget::sign(&f.world.owner, policy).ok_or("owner signature")?;
            f.world.runtime.install_policy(signed).await?;
            f.message(LATER).await?;
            let next = f.sync(false).await?;
            let other = next.runs.first().ok_or("second discovered run")?;
            let before_refusal = checkpoint(&f).await?;
            assert!(matches!(
                native::testing::reserve_retained_reply(&f.world.runtime, &other.run).await,
                Err(RuntimeError::OverBudget)
            ));
            assert_eq!(checkpoint(&f).await?, before_refusal);
            assert_eq!(
                f.world.count("SELECT count(*) FROM runtime_attempts").await,
                1
            );
            assert_eq!(f.world.balance().await.held_units, binding.hold_units);
        }
        assert_eq!(
            f.world.runtime.continuity(),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert!(f.world.runtime.production_output_scope().is_err());
        assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
        for query in [
            "SELECT count(*) FROM runtime_claims",
            "SELECT count(*) FROM runtime_admission_witnesses",
            "SELECT count(*) FROM runtime_evidence",
            "SELECT count(*) FROM runtime_dispositions",
        ] {
            assert_eq!(f.world.count(query).await, 0);
        }
        reopen(&mut f).await?;
        assert_eq!(f.world.balance().await.held_units, binding.hold_units);
        f.finish().await;
        println!("shared native reservation fault {fault}: actual native scope, original owner-wide hold/journal/postings, cold SQL-first recovery, exact replay; real older SQL ledger with matching markers and extra overspend refused, zero claim/HTTP/bill/release PASS");
    }
    Ok(())
}
