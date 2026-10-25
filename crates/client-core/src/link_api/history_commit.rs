//! The existing atomic import, under an already admitted Engine guard.
use super::{
    chunk_meta, invalid, link, t, BTreeSet, CoreError, HistoryDto, HistoryReceive, META_HISTORY,
    META_HISTORY_COMPLETE, META_INITIAL_HISTORY_PENDING,
};
use crate::engine::Engine;

pub(super) fn commit(
    e: &mut Engine,
    events: Vec<types::Event>,
    chunks: u32,
    resumed: u32,
    receive: &HistoryReceive,
) -> Result<HistoryDto, CoreError> {
    receive.check()?;
    let mut positions = BTreeSet::new();
    let mut opened = Vec::new();
    for event in events {
        receive.check()?;
        if !positions.insert((event.space.clone(), event.seq)) {
            return Err(invalid("The history repeats a relay position."));
        }
        let local = e
            .logs
            .get(&event.space)
            .and_then(|log| {
                usize::try_from(event.seq)
                    .ok()
                    .and_then(|index| log.events().get(index))
            })
            .ok_or_else(|| {
                invalid(t(
                    "O histórico aguarda os registros verificados do servidor.",
                    "History is waiting for the verified relay records.",
                ))
            })?;
        if local.sealed_wire.is_some() {
            // A retry after a former partial import must be the exact same opened event.
            let mut anchored = event;
            anchored.prev = local.prev.clone();
            anchored.hash = local.hash.clone();
            if &anchored != local {
                return Err(invalid("An opened history entry does not match."));
            }
            continue;
        }
        let event = link::opened_history_event(local, event)
            .ok_or_else(|| invalid("A history entry does not match the signed relay record."))?;
        opened.push(event);
    }
    let receipt = HistoryDto {
        messages: opened.len() as u32,
        chunks,
        resumed,
    };
    {
        let tx = e
            .store
            .conn()
            .unchecked_transaction()
            .map_err(|err| CoreError::Storage {
                message: err.to_string(),
            })?;
        for event in opened {
            receive.check()?;
            if !e.store.replace_event(&event)? {
                return Err(invalid("A history relay record disappeared."));
            }
        }
        e.store.set_meta(
            META_HISTORY_COMPLETE,
            &serde_json::to_string(&receipt).map_err(|err| invalid(err.to_string()))?,
        )?;
        for n in 0..chunks {
            e.store.meta_delete(&chunk_meta(n as usize))?;
        }
        e.store.meta_delete(META_HISTORY)?;
        e.store.meta_delete(META_INITIAL_HISTORY_PENDING)?;
        receive.check()?;
        tx.commit().map_err(|err| CoreError::Storage {
            message: err.to_string(),
        })?;
    }
    e.reload()?;
    Ok(receipt)
}
