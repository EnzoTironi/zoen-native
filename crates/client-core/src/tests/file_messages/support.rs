//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn send(
    e: &Client,
    space: &str,
    mime: &str,
    reply: Option<String>,
    thread: bool,
    id: &str,
) -> Result<TimelineEntry, CoreError> {
    e.send_file_message(
        space.into(),
        "picked.bin".into(),
        mime.into(),
        vec![7; 1024],
        Some(vec![9; 64]),
        reply,
        thread,
        id.into(),
    )
}

pub(super) fn card(entry: &TimelineEntry) -> &ItemCard {
    caption_card(entry, "")
}

pub(super) fn caption_card<'a>(entry: &'a TimelineEntry, caption: &str) -> &'a ItemCard {
    assert!(entry.voice.is_none());
    match &entry.kind {
        EntryKind::Message {
            text,
            card: Some(card),
        } => {
            assert_eq!(text, caption);
            card
        }
        other => panic!("expected file attachment, got {other:?}"),
    }
}

pub(super) fn send_caption(
    e: &Client,
    space: &str,
    reply: Option<String>,
    thread: bool,
    id: &str,
    caption: &str,
) -> Result<TimelineEntry, CoreError> {
    e.send_file_message_with_caption(
        space.into(),
        "picked.bin".into(),
        "application/pdf".into(),
        vec![7; 1024],
        Some(vec![9; 64]),
        reply,
        thread,
        id.into(),
        caption.into(),
    )
}

pub(super) fn storage(e: &Client) -> Vec<i64> {
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

pub(super) fn file_document(e: &Client, item: &str) -> types::FileDoc {
    let engine = e.lock();
    let types::ItemContent::File(doc) = &engine.state.items[item].current().content else {
        panic!("expected a file document");
    };
    doc.clone()
}
