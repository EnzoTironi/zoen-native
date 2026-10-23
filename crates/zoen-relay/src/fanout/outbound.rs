//! Logical retention credits for the application queue and its active publisher.
//! These are payload/audience/trace bounds, not allocator, SDK buffer or RSS limits.
use super::*;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(super) const MAX_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct Outbound {
    pub to: Vec<String>,
    pub bytes: Bytes,
    pub trace: Vec<(String, String)>,
    pub credits: OwnedSemaphorePermit,
}

fn logical_bytes(bytes: &Bytes, to: &[String], trace: &[(String, String)]) -> Option<u32> {
    let mut size = bytes.len().checked_add(std::mem::size_of::<Outbound>())?;
    for recipient in to {
        size = size
            .checked_add(std::mem::size_of::<String>())?
            .checked_add(recipient.len())?;
    }
    for (key, value) in trace {
        size = size
            .checked_add(std::mem::size_of::<(String, String)>())?
            .checked_add(key.len())?
            .checked_add(value.len())?;
    }
    u32::try_from(size).ok()
}

pub(super) fn enqueue(
    out: &mpsc::Sender<Outbound>,
    budget: &Arc<Semaphore>,
    to: &[String],
    frame: &ServerFrame,
    trace: impl FnOnce() -> Vec<(String, String)>,
) -> bool {
    // Reserve count before encoding, cloning recipients or capturing trace.
    let Ok(slot) = out.try_reserve() else {
        return false;
    };
    if budget.available_permits() == 0 {
        return false;
    }
    // With some remaining credit an encoding is needed to determine exact wire
    // size. A refused temporary is dropped; it never enters retained queue state.
    let bytes = Bytes::from(frame.encode());
    let trace = trace();
    let Some(size) = logical_bytes(&bytes, to, &trace) else {
        return false;
    };
    let Ok(credits) = budget.clone().try_acquire_many_owned(size) else {
        return false;
    };
    slot.send(Outbound {
        to: to.to_vec(),
        bytes,
        trace,
        credits,
    });
    true
}

#[cfg(test)]
mod tests;
