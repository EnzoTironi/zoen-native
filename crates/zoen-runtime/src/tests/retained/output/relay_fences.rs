//! Actual changes AFTER the final native/SQL/FDB checks and before Publish.
use super::*;
use crate::tests::retained::image_tasks::delivery::relay::Relay;

pub(super) async fn late_relay_fence_races() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    for change in ["cancel", "revoke", "new-head", "expired"] {
        let mut f = Fixture::with_managed_output().await?;
        let expiry = if change == "expired" {
            Some(f.world.runtime.finance.reply_clock().await? + 5000)
        } else {
            None
        };
        f.grant_until("late-relay-source", expiry).await?;
        f.message(ORIGINAL).await?;
        let run = f.sync(false).await?.runs.remove(0);
        f.world.runtime.run_model(&run.run).await?;
        proof::prepare(&f.world.runtime, &run.run).await?;
        let original = proof::publication(&f.world.runtime, &run.run).await;
        let relay = Relay::start(&f).await?;
        let target = relay.target(&f, relay.address);
        let runtime = Arc::clone(&f.world.runtime);
        let scope = native::testing::qualify_output(&runtime, target.clone()).await?;
        runtime.fault.store(73, Ordering::SeqCst);
        let native = runtime.native.as_ref().ok_or("native custody")?;
        let (delivery, changed) = tokio::join!(
            native.deliver_output(&runtime, &run.run, &target, &scope),
            async {
                let result = async {
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        runtime.native_cut_entered.notified(),
                    )
                    .await?;
                    match change {
                        "cancel" => {
                            runtime.cancel_model(&run.run).await?;
                        }
                        "revoke" => {
                            f.encrypted(EventBody::GrantRevoked {
                                grant: "late-relay-source".into(),
                            })
                            .await?;
                        }
                        "new-head" => {
                            f.message("Later source position wins before the old reply.")
                                .await?;
                        }
                        "expired" => {
                            let deadline = expiry.ok_or("missing test expiry")?;
                            let remaining = deadline
                                .saturating_sub(runtime.finance.reply_clock().await?)
                                .max(0);
                            tokio::time::sleep(Duration::from_millis(
                                u64::try_from(remaining)? + 20,
                            ))
                            .await;
                        }
                        _ => return Err("unknown race".into()),
                    }
                    Ok::<(), Box<dyn std::error::Error>>(())
                }
                .await;
                runtime.native_cut_resume.notify_one();
                result
            }
        );
        runtime.fault.store(0, Ordering::SeqCst);
        changed?;
        // Consumed external intent plus a refused append is uncertainty, never
        // permission to recreate or resend the paid original from absence.
        assert_eq!(delivery, Err(RuntimeError::RestoreUnreconciled), "{change}");
        let entries = f.log.read(&f.space, 1, 256).await?;
        assert!(
            entries.iter().all(|position| position.env != original),
            "{change}"
        );
        assert_eq!(
            relay.state.metrics.events_sequenced.load(Ordering::SeqCst),
            0
        );
        assert_eq!(proof::publication(&runtime, &run.run).await, original);
        assert_one_bill(&f).await;
        drop(scope);
        drop(runtime);
        cold(&mut f).await;
        assert_eq!(
            proof::publication(&f.world.runtime, &run.run).await,
            original
        );
        assert_one_bill(&f).await;
        relay.stop().await;
        f.world.finish().await;
        println!("managed text late {change}: change after final local authorization is conflict-checked by real relay append, zero response events, cold original and one bill retained, no absence-based resend PASS");
    }
    Ok(())
}
