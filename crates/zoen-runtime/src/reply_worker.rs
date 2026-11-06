//! Reusable bounded scheduler. Native wakes and receipts remain durable authority.
use crate::{ReplyHostProgress, ReplyReceiptProgress, ReplyRuntimeHost, RuntimeError};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
const MAX_DEFERRED: usize = 4096;
const RETRY: Duration = Duration::from_secs(2);
const REVIEW: Duration = Duration::from_secs(60);

pub enum ReplyWorkerEvent {
    Progress(Box<ReplyHostProgress>),
    Deferred { run: String, error: RuntimeError },
    ConversationDeferred { space: String, error: RuntimeError },
}
/// Cursors and cooldowns affect scheduling only. Losing them cannot spend,
/// refund, re-infer, authorize a publication or erase an original/receipt.
#[derive(Default)]
pub struct ReplyWorker {
    conversations: Option<String>,
    pending: Option<String>,
    deferred: BTreeMap<String, Instant>,
    control: Option<Option<u64>>,
}
impl ReplyWorker {
    /// Scheduling hint after a verified local owner-policy installation. This
    /// changes no durable original, grant, receipt, financial state or permit.
    pub fn reconcile_soon(&mut self) {
        self.deferred.clear();
    }

    pub async fn tick(
        &mut self,
        host: &ReplyRuntimeHost,
    ) -> Result<Vec<ReplyWorkerEvent>, RuntimeError> {
        self.observe_control(host.control_revision().await?)?;
        let now = Instant::now();
        self.deferred.retain(|_, until| *until > now);
        let mut events = Vec::new();
        let (spaces, next) = host
            .conversation_page(self.conversations.as_deref())
            .await?;
        self.conversations = next;
        for space in spaces {
            let key = format!("space:{space}");
            if self.deferred.contains_key(&key) {
                continue;
            }
            match host.discover(&space).await {
                Ok(_) => {}
                Err(error) if retryable(error) => {
                    self.defer(key, REVIEW)?;
                    events.push(ReplyWorkerEvent::ConversationDeferred { space, error });
                }
                Err(error) => return Err(error),
            }
        }
        let page = host.pending_page(self.pending.as_deref()).await?;
        self.pending = page.next;
        for run in page.runs {
            if self.deferred.contains_key(&run.run) {
                continue;
            }
            match Box::pin(host.advance(&run.run)).await {
                Ok(progress) => {
                    let delay = match &progress {
                        ReplyHostProgress::Completed {
                            delivery: ReplyReceiptProgress::Refused { .. },
                            ..
                        }
                        | ReplyHostProgress::Output {
                            delivery: ReplyReceiptProgress::Refused { .. },
                            ..
                        } => Some(REVIEW),
                        ReplyHostProgress::Completed { .. }
                        | ReplyHostProgress::Output {
                            delivery: ReplyReceiptProgress::Accepted { .. },
                            ..
                        } => None,
                        ReplyHostProgress::Held { .. }
                        | ReplyHostProgress::AwaitingRecovery { .. } => Some(REVIEW),
                        _ => Some(RETRY),
                    };
                    if let Some(delay) = delay {
                        self.defer(run.run, delay)?;
                    }
                    events.push(ReplyWorkerEvent::Progress(Box::new(progress)));
                }
                Err(error) if retryable(error) => {
                    self.defer(
                        run.run.clone(),
                        if error == RuntimeError::Unavailable {
                            RETRY
                        } else {
                            REVIEW
                        },
                    )?;
                    events.push(ReplyWorkerEvent::Deferred {
                        run: run.run,
                        error,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(events)
    }
    fn observe_control(&mut self, revision: Option<u64>) -> Result<(), RuntimeError> {
        if let Some(previous) = self.control {
            if previous.is_some() && revision < previous {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            if previous != revision {
                // Owner resume should not wait behind a policy/model cooldown.
                // This only reschedules originals; advance rechecks all authority.
                self.deferred.clear();
            }
        }
        self.control = Some(revision);
        Ok(())
    }
    fn defer(&mut self, key: String, delay: Duration) -> Result<(), RuntimeError> {
        if self.deferred.len() >= MAX_DEFERRED && !self.deferred.contains_key(&key) {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        self.deferred.insert(key, Instant::now() + delay);
        Ok(())
    }
}
fn retryable(error: RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::Denied | RuntimeError::OverBudget | RuntimeError::Unavailable
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_control_changes_reschedule_without_accepting_revision_rollback() {
        let mut worker = ReplyWorker::default();
        assert!(worker.observe_control(Some(1)).is_ok());
        assert!(worker.defer("paid-original".into(), REVIEW).is_ok());
        assert!(worker.observe_control(Some(1)).is_ok());
        assert_eq!(worker.deferred.len(), 1);
        assert!(worker.observe_control(Some(2)).is_ok());
        assert!(worker.deferred.is_empty());
        assert_eq!(
            worker.observe_control(Some(1)),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert_eq!(
            worker.observe_control(None),
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert_eq!(worker.control, Some(Some(2)));
    }
    #[test]
    fn cooldowns_are_bounded_and_authority_failures_never_retry_as_success() {
        let mut worker = ReplyWorker::default();
        for index in 0..MAX_DEFERRED {
            assert!(worker.defer(index.to_string(), REVIEW).is_ok());
        }
        assert_eq!(
            worker.defer("extra".into(), REVIEW),
            Err(RuntimeError::EvidenceTooLarge)
        );
        assert!(worker.defer("0".into(), REVIEW).is_ok());
        assert!(!retryable(RuntimeError::RestoreUnreconciled));
        assert!(!retryable(RuntimeError::InvalidBinding));
    }
}
