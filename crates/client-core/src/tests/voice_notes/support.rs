//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn ingest_as(
    engine: &mut crate::engine::Engine,
    space: &str,
    author: &str,
    body: EventBody,
) {
    let event = engine.author_for(author).unwrap().sign_event(
        space,
        &types::new_id("voice-event"),
        crate::engine::now_ms(),
        engine.logs[space].head(),
        body,
    );
    let mut relay = engine.logs[space].clone();
    let event = relay.sequence(event);
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event),
        }),
        crate::sync::Ingest::Applied
    );
}

pub(super) fn note(
    e: &Client,
    space: &str,
    reply: Option<String>,
) -> Result<TimelineEntry, CoreError> {
    e.send_voice_note(
        space.into(),
        vec![7; 900],
        "audio/mp4".into(),
        1500,
        vec![0.2],
        "A recording".into(),
        reply,
        true,
    )
}

pub(super) fn last(e: &Client, space: &str) -> TimelineEntry {
    e.timeline(space.into()).unwrap().pop().unwrap()
}

/// A complete mono PCM WAV, so forwarding tests carry a decodable audio container.
pub(super) fn pcm_wav(frames: u32) -> Vec<u8> {
    let length = frames * 2;
    let mut wav = Vec::with_capacity(length as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wav.extend_from_slice(&8000u32.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&length.to_le_bytes());
    for index in 0..frames {
        let sample = ((index % 80) as i16 - 40) * 400;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    assert_eq!(wav.len(), length as usize + 44);
    wav
}

pub(super) fn wav_note(
    e: &Client,
    space: &str,
    bytes: &[u8],
    reply: Option<String>,
    thread: bool,
) -> TimelineEntry {
    e.send_voice_note(
        space.into(),
        bytes.to_vec(),
        "audio/wav".into(),
        ((bytes.len() - 44) / 16) as u64,
        vec![0.0, 0.5, 1.0],
        "Synthetic audio transcript".into(),
        reply,
        thread,
    )
    .unwrap()
}

pub(super) fn voice_document(e: &Client, item: &str) -> types::FileDoc {
    let engine = e.lock();
    let ItemContent::File(document) = &engine.state.items[item].current().content else {
        panic!("voice file")
    };
    document.clone()
}

pub(super) fn voice_forward_storage(e: &Client) -> Vec<i64> {
    let engine = e.lock();
    [
        "events",
        "meta",
        "media",
        "media_keys",
        "blob_uploads",
        "outbox",
    ]
    .map(|table| {
        engine
            .store
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .to_vec()
}
