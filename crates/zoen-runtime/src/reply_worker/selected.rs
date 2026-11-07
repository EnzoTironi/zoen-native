//! One selected source, using the same cooldowns and canonical original index.
use super::*;
use crate::ReplySource;

pub struct SelectedReplyWorker {
    source: ReplySource,
    original: Option<String>,
    schedule: ReplyWorker,
}
impl SelectedReplyWorker {
    /// The selector is data; every tick resolves native permission independently.
    pub fn new(source: ReplySource) -> Result<Self, RuntimeError> {
        source.validate()?;
        Ok(Self {
            source,
            original: None,
            schedule: ReplyWorker::default(),
        })
    }
    pub fn reconcile_soon(&mut self) {
        self.schedule.reconcile_soon();
    }
    /// Returns complete only for an actual accepted final answer to this source.
    pub async fn tick(
        &mut self,
        host: &ReplyRuntimeHost,
    ) -> Result<(Vec<ReplyWorkerEvent>, bool), RuntimeError> {
        self.schedule
            .observe_control(host.control_revision().await?)?;
        let key = "selected-original";
        let now = Instant::now();
        self.schedule.deferred.retain(|_, until| *until > now);
        if self.schedule.deferred.contains_key(key) {
            return Ok((Vec::new(), false));
        }
        let result = self.advance(host).await;
        match result {
            Ok(Some(progress)) => {
                let complete = matches!(
                    &progress,
                    ReplyHostProgress::Output {
                        delivery: ReplyReceiptProgress::Accepted { .. },
                        ..
                    } | ReplyHostProgress::Completed {
                        delivery: ReplyReceiptProgress::Accepted { .. },
                        ..
                    }
                );
                if !complete {
                    let delay = match &progress {
                        ReplyHostProgress::Held { .. }
                        | ReplyHostProgress::AwaitingRecovery { .. } => REVIEW,
                        ReplyHostProgress::Review { status, .. }
                            if *status != client_core::RequestStatus::Pending =>
                        {
                            REVIEW
                        }
                        _ => RETRY,
                    };
                    self.schedule.defer(key.into(), delay)?;
                }
                Ok((
                    vec![ReplyWorkerEvent::Progress(Box::new(progress))],
                    complete,
                ))
            }
            Ok(None) => {
                self.schedule.defer(key.into(), RETRY)?;
                Ok((Vec::new(), false))
            }
            Err(error) if retryable(error) => {
                self.schedule.defer(
                    key.into(),
                    if error == RuntimeError::Unavailable {
                        RETRY
                    } else {
                        REVIEW
                    },
                )?;
                let event = match &self.original {
                    Some(run) => ReplyWorkerEvent::Deferred {
                        run: run.clone(),
                        error,
                    },
                    None => ReplyWorkerEvent::ConversationDeferred {
                        space: self.source.space.clone(),
                        error,
                    },
                };
                Ok((vec![event], false))
            }
            Err(error) => Err(error),
        }
    }
    async fn advance(
        &mut self,
        host: &ReplyRuntimeHost,
    ) -> Result<Option<ReplyHostProgress>, RuntimeError> {
        let selected = host.discover_source(&self.source).await?;
        if !selected.caught_up {
            return Ok(None);
        }
        let [run] = selected.runs.as_slice() else {
            return Err(RuntimeError::InvalidBinding);
        };
        if self
            .original
            .as_ref()
            .is_some_and(|original| original != &run.run)
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        self.original = Some(run.run.clone());
        Ok(Some(
            Box::pin(host.advance_source(&run.run, &self.source)).await?,
        ))
    }
}
