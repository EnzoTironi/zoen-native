//! Reserve bounded queue capacity before consuming an independent effect.
use super::*;
use relay_admission::managed_images::{
    AnchorReader, Checkpoint, ConsumePublish, ManagedPublishPermit, PublishCondition,
};

pub(crate) enum ManagedAdmission {
    Queued(Box<QueuedManaged>),
    Consumed(Box<PublishCondition>),
}

pub(crate) struct QueuedManaged {
    reservation: QueueReservation,
    permit: ManagedPublishPermit,
}

impl QueuedManaged {
    pub(crate) async fn append(self: Box<Self>) -> Reply {
        let Self {
            reservation,
            permit,
        } = *self;
        reservation.send(Some(Arc::new(permit))).await
    }
}

pub(super) struct QueueReservation {
    pending: Pending,
    slot: mpsc::OwnedPermit<Job>,
    count: OwnedSemaphorePermit,
    bytes: OwnedSemaphorePermit,
}

fn busy() -> Reject {
    Reject::retry("sequencer capacity reached")
}

impl<B: Batcher> Sequencer<B> {
    /// Local ownership must already be established. No independent consumption
    /// occurs if count, bytes, queue slots or worker admission refuse capacity.
    pub(crate) fn admit_managed(
        &self,
        env: Envelope,
        target_known: bool,
        fence: Fence,
        reader: &AnchorReader,
        checkpoint: &Checkpoint,
    ) -> Result<ManagedAdmission, Reject> {
        let reservation = self.reserve(env, target_known, fence)?;
        match reader
            .consume_publish(checkpoint, &reservation.pending.env)
            .map_err(|_| Reject::retry("managed publication refused"))?
        {
            ConsumePublish::Fresh(permit) => {
                Ok(ManagedAdmission::Queued(Box::new(QueuedManaged {
                    reservation,
                    permit,
                })))
            }
            ConsumePublish::Consumed(condition) => {
                Ok(ManagedAdmission::Consumed(Box::new(condition)))
            }
        }
    }

    pub(super) fn reserve(
        &self,
        env: Envelope,
        target_known: bool,
        fence: Fence,
    ) -> Result<QueueReservation, Reject> {
        let count = self.count.clone().try_acquire_owned().map_err(|_| busy())?;
        let size = env.retained_len().saturating_add(512);
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(size.min(u32::MAX as usize) as u32)
            .map_err(|_| busy())?;
        let space = env.space().to_string();
        // A worker that just closed is removed only if this is still its entry.
        for attempt in 0..2 {
            let sender = self.queue(&space)?;
            match sender.clone().try_reserve_owned() {
                Ok(slot) => {
                    return Ok(QueueReservation {
                        pending: Pending {
                            env,
                            target_known,
                            fence,
                            managed: None,
                        },
                        slot,
                        count,
                        bytes,
                    });
                }
                Err(mpsc::error::TrySendError::Full(_)) => return Err(busy()),
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    if attempt == 1 {
                        return Err(busy());
                    }
                    let mut queues = self.queues.lock().map_err(|_| Reject::unavailable())?;
                    if queues
                        .get(&space)
                        .is_some_and(|queue| queue.same_channel(&sender))
                    {
                        queues.remove(&space);
                    }
                }
            }
        }
        Err(busy())
    }
}

impl QueueReservation {
    pub(super) async fn send(mut self, managed: Option<Arc<ManagedPublishPermit>>) -> Reply {
        self.pending.managed = managed;
        let (reply, answer) = oneshot::channel();
        self.slot.send(Job {
            pending: self.pending,
            reply,
            _count: self.count,
            _bytes: self.bytes,
        });
        answer.await.unwrap_or((
            Err(Reject::unavailable()),
            BatchStats {
                size: 0,
                attempts: 0,
            },
        ))
    }
}
