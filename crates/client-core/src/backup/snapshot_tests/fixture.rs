//! fixture scenarios; existing shared fixture remains in the parent.
use super::*;

pub(super) fn canonical(engine: &Engine, identities: &[String]) -> Canonical {
    let timelines: Vec<_> = SPACES
        .iter()
        .map(|id| engine.timeline(id).unwrap())
        .collect();
    for (space, entries) in SPACES.iter().zip(&timelines) {
        let count = MESSAGES + 2 * usize::from(*space == SPACES[0]);
        assert_eq!(
            entries.len(),
            count,
            "no skipped or duplicated rows in {space}"
        );
        let mut ids: Vec<_> = (0..MESSAGES)
            .map(|i| format!("{space}-message-{i:02}"))
            .collect();
        if *space == SPACES[0] {
            ids.extend(["backup-file-edit".into(), "backup-file-message".into()]);
        }
        assert_eq!(
            entries.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
            ids.iter().collect::<Vec<_>>()
        );
        assert_eq!(entries[0].at_ms, AT + 2);
        assert_eq!(entries[0].seq, 2);
        assert!(
            matches!(&entries[0].kind, EntryKind::Message { text, card: None } if text == FINAL_TEXT)
        );
        assert!(entries[0].edited);
        assert_eq!(entries[1].reply_to.as_ref().unwrap().id, entries[0].id);
        assert_eq!(entries[1].reply_to.as_ref().unwrap().text, FINAL_TEXT);
        assert_eq!(
            entries[2].in_thread.as_deref(),
            Some(entries[0].id.as_str())
        );
        assert_eq!(entries[0].thread_replies, 1);
        assert!(entries[3].forwarded);
        assert!(entries[4].deleted);
        assert!(
            matches!(&entries[4].kind, EntryKind::Message { text, card: None } if text.is_empty())
        );
        assert_eq!(entries[0].reactions.len(), 1);
        assert_eq!(entries[0].reactions[0].emoji, "🧭");
        let thread = engine.thread(space, &entries[0].id).unwrap();
        assert_eq!(thread.len(), 2);
        assert_eq!(thread, [entries[0].clone(), entries[2].clone()]);
        assert!(engine.verify_log(space).valid);
    }
    let mut items = engine.items();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, FILE);
    assert_eq!(items[0].versions.len(), 2);
    assert_eq!(items[0].version, 2);
    assert_eq!(items[0].versions[0].note, "Revised route");
    Canonical {
        spaces: SPACES
            .iter()
            .map(|id| engine.space_summary(id).unwrap())
            .collect(),
        timelines,
        items,
        profiles: identities
            .iter()
            .map(|id| engine.profile_view(id).unwrap())
            .collect(),
    }
}

pub(super) fn push(
    engine: &mut Engine,
    log: &mut SpaceLog,
    author: &Author,
    id: &str,
    body: EventBody,
) {
    let signed = author.sign_event(log.space(), id, AT + log.len() as i64, log.head(), body);
    let event = log.sequence(signed);
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event),
        }),
        Ingest::Applied
    );
}

pub(super) fn file(bytes: &[u8], version: u8) -> (FileDoc, Vec<u8>) {
    // Fixed, distinct keys/nonces are synthetic fixture input only; no production RNG substitute.
    let key = [80 + version; 32];
    let nonce = [90 + version; 24];
    let mut sealed = nonce.to_vec();
    sealed.extend(
        XChaCha20Poly1305::new_from_slice(&key)
            .unwrap()
            .encrypt(&XNonce::from(nonce), bytes)
            .unwrap(),
    );
    let sha256 = hex::encode(Sha256::digest(bytes));
    (
        FileDoc {
            name: "route.pdf".into(),
            path: "Trips/route.pdf".into(),
            mime: "application/pdf".into(),
            bytes: bytes.len() as u64,
            sha256: sha256.clone(),
            chunks: vec![ChunkRef {
                sha256,
                bytes: bytes.len() as u64,
                blob: hex::encode(Sha256::digest(&sealed)),
                key: hex::encode(key),
            }],
            thumb: None,
            voice: None,
        },
        sealed,
    )
}
