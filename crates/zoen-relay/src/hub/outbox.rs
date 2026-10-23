//! One session's encoded mailbox and active writer share retention credits.
//! This bounds application retention, not transient encodings, WebSocket buffers or RSS.
use std::sync::Arc;

use bytes::Bytes;
use protocol::ServerFrame;
use tokio::sync::{mpsc, Notify, OwnedSemaphorePermit, Semaphore};

const FRAME_CAPACITY: usize = 1024;
const BYTE_CAPACITY: usize = 32 * 1024 * 1024;
type Refusal = mpsc::error::TrySendError<()>;

/// The permit stays with the active writer until send completion or cancellation.
pub(crate) struct Envelope {
    pub(crate) bytes: Bytes,
    _credits: OwnedSemaphorePermit,
}

#[derive(Clone)]
pub struct Outbox {
    tx: mpsc::Sender<Envelope>,
    budget: Arc<Semaphore>,
    kick: Arc<Notify>,
}

pub(crate) fn channel(kick: Arc<Notify>) -> (Outbox, mpsc::Receiver<Envelope>) {
    channel_with_limits(FRAME_CAPACITY, BYTE_CAPACITY, kick)
}

pub(super) fn channel_with_limits(
    frames: usize,
    bytes: usize,
    kick: Arc<Notify>,
) -> (Outbox, mpsc::Receiver<Envelope>) {
    let (tx, rx) = mpsc::channel(frames);
    (
        Outbox {
            tx,
            budget: Arc::new(Semaphore::new(bytes)),
            kick,
        },
        rx,
    )
}

impl Outbox {
    /// Preserve direct-response count backpressure. Byte overflow disconnects the
    /// session instead of retaining another frame or silently losing its response.
    pub async fn send(&self, frame: ServerFrame) -> Result<(), mpsc::error::SendError<()>> {
        let slot = self
            .tx
            .reserve()
            .await
            .map_err(|_| mpsc::error::SendError(()))?;
        self.enqueue(slot, || Bytes::from(frame.encode()))
            .map_err(|_| mpsc::error::SendError(()))
    }

    /// The Hub shares one lazy encoding across recipients, after count admission.
    pub(crate) fn try_send_encoded(&self, encode: impl FnOnce() -> Bytes) -> Result<(), Refusal> {
        let slot = self.tx.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Refusal::Full(()),
            mpsc::error::TrySendError::Closed(_) => Refusal::Closed(()),
        })?;
        self.enqueue(slot, encode)
    }

    fn enqueue(
        &self,
        slot: mpsc::Permit<'_, Envelope>,
        encode: impl FnOnce() -> Bytes,
    ) -> Result<(), Refusal> {
        let refuse = || {
            // Existing disconnect/Sync recovery repairs missed durable events.
            self.kick.notify_one();
            Refusal::Full(())
        };
        if self.budget.available_permits() == 0 {
            return Err(refuse());
        }
        let bytes = encode();
        let size = bytes
            .len()
            .checked_add(std::mem::size_of::<Envelope>())
            .and_then(|size| u32::try_from(size).ok())
            .ok_or_else(refuse)?;
        let credits = self
            .budget
            .clone()
            .try_acquire_many_owned(size)
            .map_err(|_| refuse())?;
        slot.send(Envelope {
            bytes,
            _credits: credits,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
