//! Match the real relay's (Space, author, CID) idempotency contract.
use protocol::{Envelope, Sequenced};

pub(super) fn sequence(
    log: &mut Vec<Sequenced>,
    env: Envelope,
) -> Result<Sequenced, serde_json::Error> {
    // The per-Space log is held under the same mutex during lookup and append.
    // A lost Event/Accepted or late old socket must return the stored receipt,
    // never assign the same signed intent a second sequence and strand its retry.
    if let Some(original) = log.iter().find(|event| {
        event.env.author() == env.author() && event.env.client_id() == env.client_id()
    }) {
        return Ok(original.clone());
    }
    let seq = log.len() as u64;
    let prev = log
        .last()
        .map(|event| event.hash.clone())
        .unwrap_or_else(|| types::GENESIS_PREV.into());
    let event = Sequenced {
        seq,
        hash: event_log::chain_hash(env.space(), seq, &prev, &env.wire_hash())?,
        prev,
        env,
    };
    log.push(event.clone());
    Ok(event)
}
