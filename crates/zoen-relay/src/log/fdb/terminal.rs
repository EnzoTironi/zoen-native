//! The upstream runner returned a committed batch. Preserve any earlier
//! maybe-committed result as Unknown rather than manufacturing a negative.
use super::*;

#[derive(Default)]
pub(super) struct History(std::sync::atomic::AtomicBool);
impl History {
    pub(super) fn observe(&self, maybe_committed: bool) {
        if maybe_committed {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    pub(super) fn uncertain(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub(super) async fn confirm(
    batch: &[Pending],
    mut results: Vec<Result<Sequencing, Reject>>,
    uncertain: bool,
) -> Vec<Result<Sequencing, Reject>> {
    if uncertain {
        return results;
    }
    for (pending, result) in batch.iter().zip(&mut results) {
        let Err(rejected) = result else {
            continue;
        };
        let (Some(reason), Some(permit)) = (rejected.managed_refusal, pending.managed.as_ref())
        else {
            continue;
        };
        let permit = Arc::clone(permit);
        let confirmed =
            tokio::task::spawn_blocking(move || permit.record_committed_refusal(reason)).await;
        if matches!(confirmed, Ok(Ok(()))) {
            rejected.permanent = true;
        } else {
            // Never expose a known terminal outcome before independent fsync.
            *rejected = Reject::unavailable();
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_admission::managed_images::{
        publish_digest, AnchorWriter, ConsumePublish, Deployment, PublishRefusal,
    };

    struct Home(std::path::PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn one_uncertain_attempt_cannot_be_erased_by_a_later_known_retry() {
        let history = History::default();
        history.observe(false);
        assert!(!history.uncertain());
        history.observe(true);
        history.observe(false);
        assert!(history.uncertain());
    }

    #[tokio::test]
    async fn only_known_committed_refusals_become_independent_terminal_outcomes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for uncertain in [false, true] {
            let home = Home(std::env::temp_dir().join(types::new_id("terminal-proof")?));
            let actor = event_log::Author::device(
                &event_log::Signer::generate()?,
                event_log::Signer::generate()?,
            );
            let env = Envelope::sealed(
                &actor,
                "space",
                "original",
                1,
                None,
                protocol::Sealed::new(protocol::SealedKind::Application, 1, vec![5; 32]),
            )?;
            let deployment = Deployment {
                namespace: "terminal-proof".into(),
                source: "cell".into(),
                witness: "ab".repeat(32),
                agent: actor.identity.clone(),
                device: actor.device.clone().ok_or("certified device")?,
            };
            let mut writer = AnchorWriter::create(&home.0, deployment.clone())?;
            let genesis = writer.reader().checkpoint()?;
            let prepared = writer.prepare(
                &publish_digest(&deployment.namespace, &env)?,
                &genesis,
                &genesis,
                false,
            )?;
            writer.reconcile(&prepared.next, &prepared.next)?;
            let ConsumePublish::Fresh(permit) =
                writer.reader().consume_publish(&prepared.next, &env)?
            else {
                return Err("missing first permit".into());
            };
            let permit = Arc::new(permit);
            let pending = Pending {
                env: env.clone(),
                target_known: true,
                fence: crate::ownership::Fence {
                    partition: crate::ownership::partition_of("space"),
                    owner: "owner".into(),
                    token: 1,
                },
                managed: Some(Arc::clone(&permit)),
            };
            let results = confirm(
                &[pending],
                vec![Err(Reject::managed_candidate(PublishRefusal::Cancelled))],
                uncertain,
            )
            .await;
            let Some(Err(refused)) = results.first() else {
                return Err("missing rejection".into());
            };
            assert_eq!(refused.permanent, !uncertain);
            assert_eq!(
                writer.reader().publication_refusal(&prepared.next, &env)?,
                if uncertain {
                    None
                } else {
                    Some(PublishRefusal::Cancelled)
                }
            );
            assert_eq!(permit.check(&env).is_ok(), uncertain);
            assert!(matches!(
                writer.reader().consume_publish(&prepared.next, &env)?,
                ConsumePublish::Consumed(_)
            ));
        }
        Ok(())
    }
}
