//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn refused(f: &Fixture, ids: &[&str], grouping: &str) -> MediaDraftSendResult {
    media_contract_confirm_scope(f);
    let draft = save(
        f,
        "0",
        &media_options(ids, "file", grouping),
        ids.iter().map(|id| input(id)).collect(),
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let file = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| {
            matches!(
                &row.event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            )
        })
        .unwrap()
        .event
        .client_id;
    assert!(f
        .engine
        .lock()
        .reject(&file, "synthetic permanent refusal", true, None)
        .user_failed());
    for entry in &sent.entries {
        assert_eq!(
            f.engine
                .timeline(f.scope.space_id.clone())
                .unwrap()
                .iter()
                .find(|row| row.id == entry.id)
                .unwrap()
                .delivery,
            crate::Delivery::Failed
        );
    }
    sent
}

pub(super) fn recover(f: &Fixture, id: &str) -> R<MediaDraftDto> {
    let media = read(f);
    let engine = f.engine.lock();
    let normal = engine.message_draft(&f.scope.normal())?;
    engine.recover_failed_media_draft(&f.scope, id, &media.revision, &normal.revision)
}

pub(super) fn confirm_failed_id(f: &Fixture, id: &str) {
    let mut engine = f.engine.lock();
    let pending = engine.store.outbox_get(id).unwrap().unwrap();
    let mut relay = engine.logs[&pending.space].clone();
    let event = relay.sequence(pending);
    assert!(matches!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event)
        }),
        crate::sync::Ingest::Confirmed | crate::sync::Ingest::Applied
    ));
    assert!(engine.logs[&f.scope.space_id]
        .events()
        .iter()
        .any(|event| event.client_id == id));
}

pub(super) fn public_state(f: &Fixture) -> serde_json::Value {
    let engine = f.engine.lock();
    let mut statement = engine
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key NOT LIKE 'media.draft.%' ORDER BY key")
        .unwrap();
    let meta = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let outbox = engine.store.outbox().unwrap().into_iter().map(|row| serde_json::json!({"event":row.event,"failed":row.failed,"attempts":row.attempts,"error":row.last_error})).collect::<Vec<_>>();
    serde_json::json!({"meta":meta,"outbox":outbox,"events":engine.store.event_count().unwrap()})
}
