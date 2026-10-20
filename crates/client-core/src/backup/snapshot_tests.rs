//! Original canonical snapshot corpus: signed history, portable keys and new-device reopen.
use super::*;
use crate::dto::{EntryKind, ItemDetail, SpaceSummary, TimelineEntry};
use crate::profile::ProfileDto;
use crate::sync::Ingest;
use event_log::{
    profile::{AgreementKey, ProfileFields},
    Author, Signer, SpaceLog,
};
use rusqlite::{types::Value, Connection};
use std::path::PathBuf;
use types::{
    Annotation, ChunkRef, EventBody, FileDoc, Identity, IdentityKind, ItemContent, ItemKind,
    Privacy, ReplyRef, Role, SpaceKind,
};

const AT: i64 = 1_700_000_000_000;
const SPACES: [&str; 2] = ["sp_backup_trip", "sp_backup_team"];
const MESSAGES: usize = 32;
const FILE: &str = "it_backup_route";
const FINAL_TEXT: &str = "Plano atualizado — sábado às 08:30 🌄\nLevar água";
const FIRST_BYTES: &[u8] = b"%PDF-1.7\noriginal synthetic route\n";
const LAST_BYTES: &[u8] = b"%PDF-1.7\nrevised synthetic route with mark\n";

struct Database(PathBuf);
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("backup-canonical"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn rows(conn: &Connection, table: &str, order: &str) -> Vec<Vec<Value>> {
    let mut query = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
        .unwrap();
    let columns = query.column_count();
    query
        .query_map([], |row| {
            (0..columns)
                .map(|column| row.get::<_, Value>(column))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

type TableRows = Vec<(&'static str, Vec<Vec<Value>>)>;

fn portable_rows(conn: &Connection) -> TableRows {
    [
        ("identities", "id"),
        ("events", "space, seq"),
        ("meta", "key"),
        ("profile_keys", "identity"),
        ("peer_agreement_keys", "identity"),
        ("blocked", "identity"),
        ("synced_spaces", "space"),
        ("media_keys", "plain_sha"),
    ]
    .into_iter()
    .map(|(table, order)| {
        let mut values = rows(conn, table, order);
        if table == "meta" {
            // A restore deliberately certifies a different device, not a different account.
            values.retain(|row| row[0] != Value::Text("account".into()));
        }
        (table, values)
    })
    .collect()
}

fn assert_portable_rows(conn: &Connection, expected: &TableRows, phase: &str) {
    let actual = portable_rows(conn);
    assert_eq!(
        actual.len(),
        expected.len(),
        "{phase}: complete table cardinality"
    );
    for ((table, values), (expected_table, expected_values)) in actual.iter().zip(expected) {
        assert_eq!(table, expected_table, "{phase}: table order");
        assert_eq!(
            values.len(),
            expected_values.len(),
            "{phase}: exact {table} row count"
        );
        assert_eq!(
            values, expected_values,
            "{phase}: all ordered {table} fields"
        );
    }
}

#[derive(Debug, PartialEq)]
struct Canonical {
    spaces: Vec<SpaceSummary>,
    timelines: Vec<Vec<TimelineEntry>>,
    items: Vec<ItemDetail>,
    profiles: Vec<ProfileDto>,
}

fn canonical(engine: &Engine, identities: &[String]) -> Canonical {
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

fn push(engine: &mut Engine, log: &mut SpaceLog, author: &Author, id: &str, body: EventBody) {
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

fn file(bytes: &[u8], version: u8) -> (FileDoc, Vec<u8>) {
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

#[test]
fn canonical_backup_restores_every_row_and_projection_across_new_device_and_cold_reopen() {
    let database = Database::new();
    let source_path = database.path("source.sqlite");
    let restored_path = database.path("restored.sqlite");
    let root = Signer::from_secret(&[51; 32]);
    let peer = Signer::from_secret(&[52; 32]);
    let blocked = Signer::from_secret(&[53; 32]);
    let old_device = Signer::from_secret(&[54; 32]);
    let new_device = Signer::from_secret(&[55; 32]);
    let identities = [root.id(), peer.id(), blocked.id()];
    let mut source = Engine::open(&source_path).unwrap();
    for (index, (signer, name)) in [(&root, "Owner"), (&peer, "Peer"), (&blocked, "Blocked")]
        .into_iter()
        .enumerate()
    {
        source
            .store
            .put_identity(
                &Identity {
                    id: signer.id(),
                    kind: IdentityKind::Person,
                    name: name.into(),
                    handle: name.to_lowercase(),
                    tint_hex: "#246844".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: "Synthetic backup fixture".into(),
                },
                None,
            )
            .unwrap();
        let fields = ProfileFields {
            name: format!("{name} — perfil privado"),
            bio: "Bio 🇧🇷\nsegundo parágrafo".into(),
            photo: None,
        };
        source
            .store
            .put_profile_key(&signer.id(), &hex::encode([61 + index as u8; 32]), 3)
            .unwrap();
        source
            .store
            .set_profile_fields(&signer.id(), 5, &fields.to_bytes())
            .unwrap();
        source
            .store
            .put_peer_agreement_key(
                &signer.id(),
                &AgreementKey::from_secret([71 + index as u8; 32]).public_hex(),
            )
            .unwrap();
    }
    source.store.block(&blocked.id(), AT - 1).unwrap();
    source
        .install_restored_account(&root, old_device.clone(), "http://127.0.0.1:1")
        .unwrap();
    let owner = Author::root(root.clone());
    let peer_author = Author::root(peer.clone());
    let (first, first_sealed) = file(FIRST_BYTES, 1);
    let (last, last_sealed) = file(LAST_BYTES, 2);
    for space in SPACES {
        let mut log = SpaceLog::new(space);
        push(
            &mut source,
            &mut log,
            &owner,
            &format!("{space}-create"),
            EventBody::SpaceCreated {
                title: format!("Backup {space}"),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        );
        push(
            &mut source,
            &mut log,
            &owner,
            &format!("{space}-join"),
            EventBody::MemberAdded {
                identity: peer.id(),
                role: Role::Member,
            },
        );
        for i in 0..MESSAGES {
            let id = format!("{space}-message-{i:02}");
            let reply = matches!(i, 1 | 2).then(|| ReplyRef {
                to: format!("{space}-message-00"),
                thread: i == 2,
            });
            push(
                &mut source,
                &mut log,
                if i % 2 == 0 { &owner } else { &peer_author },
                &id,
                EventBody::MessagePosted {
                    message: format!("body-{id}"),
                    text: format!(
                        "Registro {i:02} — café 🌤️\n{}",
                        "ação/段落/line ".repeat(160)
                    ),
                    attaches: None,
                    reply,
                    forwarded: i == 3,
                },
            );
        }
        for (id, author, target, note) in [
            (
                "edit",
                &owner,
                0,
                Annotation::Edit {
                    text: FINAL_TEXT.into(),
                },
            ),
            ("delete", &owner, 4, Annotation::Delete),
            (
                "react",
                &peer_author,
                0,
                Annotation::React {
                    emoji: "🧭".into()
                },
            ),
        ] {
            push(
                &mut source,
                &mut log,
                author,
                &format!("{space}-{id}"),
                EventBody::MessageAnnotated {
                    target: format!("{space}-message-{target:02}"),
                    note,
                },
            );
        }
        if space == SPACES[0] {
            push(
                &mut source,
                &mut log,
                &owner,
                "backup-file-create",
                EventBody::ItemCreated {
                    item: FILE.into(),
                    kind: ItemKind::File,
                    content: ItemContent::File(first.clone()),
                    origin: "Synthetic attachment".into(),
                },
            );
            push(
                &mut source,
                &mut log,
                &owner,
                "backup-file-edit",
                EventBody::ItemVersioned {
                    item: FILE.into(),
                    content: ItemContent::File(last.clone()),
                    note: "Revised route".into(),
                },
            );
            push(
                &mut source,
                &mut log,
                &owner,
                "backup-file-message",
                EventBody::MessagePosted {
                    message: "body-backup-file".into(),
                    text: "Rota atualizada 🧭\nPreservar legenda".into(),
                    attaches: Some(FILE.into()),
                    reply: None,
                    forwarded: false,
                },
            );
        }
        source.mark_read(space).unwrap();
    }
    source
        .store
        .set_meta(&format!("file_key:{FILE}"), &hex::encode([79; 32]))
        .unwrap();
    for (doc, sealed) in [(&first, &first_sealed), (&last, &last_sealed)] {
        let chunk = &doc.chunks[0];
        source
            .store
            .seal_media(&chunk.sha256, &chunk.blob, &chunk.key, sealed, AT)
            .unwrap();
    }
    let expected = canonical(&source, &identities);
    let expected_rows = portable_rows(source.store.conn());
    assert!(
        expected_rows.iter().all(|(_, values)| !values.is_empty()),
        "every portable table has representative content"
    );
    assert_eq!(source.store.event_count().unwrap(), 77);
    // An unconfirmed device-local send is intentionally not part of portable confirmed history.
    let pending = owner.sign_event(
        SPACES[0],
        "pending-private-send",
        AT + 100,
        source.logs[SPACES[0]].head(),
        EventBody::MessagePosted {
            message: "pending-body".into(),
            text: "Unsent on the original device".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
    );
    source.store.outbox_put(&pending).unwrap();
    let source_outbox = rows(source.store.conn(), "outbox", "client_id");
    let source_uploads = rows(source.store.conn(), "blob_uploads", "blob_sha");
    assert_eq!(source_outbox.len(), 1);
    assert_eq!(source_uploads.len(), 2);
    let snapshot = source.backup_snapshot().unwrap();
    assert_portable_rows(source.store.conn(), &expected_rows, "source after export");
    assert_eq!(
        rows(source.store.conn(), "outbox", "client_id"),
        source_outbox
    );
    assert_eq!(
        rows(source.store.conn(), "blob_uploads", "blob_sha"),
        source_uploads
    );
    let snapshot_path = database.path("snapshot.sqlite");
    std::fs::write(&snapshot_path, &snapshot).unwrap();
    let snapshot_db = Connection::open(snapshot_path).unwrap();
    assert_portable_rows(&snapshot_db, &expected_rows, "exported snapshot");
    let page_count: u32 = snapshot_db
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    assert!(
        page_count > 32,
        "corpus must span meaningful SQLite page boundaries, got {page_count}"
    );
    eprintln!("canonical backup corpus: 77 signed events, 65 posted messages, {page_count} SQLite pages, {} snapshot bytes", snapshot.len());
    assert_eq!(
        snapshot_db
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key = 'account'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "the old device account record is excluded from the exported snapshot"
    );
    for table in ["media", "outbox", "blob_uploads", "peer_receipts"] {
        assert_eq!(
            snapshot_db
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0,
            "{table} is device-local"
        );
    }
    drop(snapshot_db);
    let header = Header {
        version: 1,
        identity: root.id(),
        identity_secret: hex::encode(root.secret()),
        agreement_secret: hex::encode([71; 32]),
        relay_url: "http://127.0.0.1:1".into(),
        created_ms: AT + 200,
    };
    let key = [78; 32];
    let sealed = seal_payload(&key, &header, &snapshot).unwrap();
    let (opened_header, opened_snapshot) = open_payload(&key, &root.id(), &sealed).unwrap();
    assert_eq!(
        serde_json::to_value(&opened_header).unwrap(),
        serde_json::to_value(&header).unwrap()
    );
    assert_eq!(opened_snapshot, snapshot);
    let mut restored = Engine::open(&restored_path).unwrap();
    assert_eq!(
        restored
            .restore_snapshot(&opened_header, &opened_snapshot, new_device.clone())
            .unwrap(),
        new_device.secret()
    );
    for phase in ["restored", "cold reopened"] {
        assert_eq!(restored.account().unwrap().identity, root.id());
        assert_eq!(restored.account().unwrap().device, new_device.id());
        assert_ne!(restored.account().unwrap().device, old_device.id());
        assert_portable_rows(restored.store.conn(), &expected_rows, phase);
        assert_eq!(
            canonical(&restored, &identities),
            expected,
            "{phase}: exact complete DTOs/order/content"
        );
        assert!(restored.store.outbox().unwrap().is_empty());
        assert_eq!(restored.uploads_pending(), 0);
        for version in [1, 2] {
            assert_eq!(
                restored.file_bytes(FILE, Some(version)).unwrap(),
                None,
                "metadata and recovery keys are portable; cached plaintext is not"
            );
        }
        drop(restored);
        restored = Engine::open(&restored_path).unwrap();
    }
    // Old and current immutable versions remain independently recoverable with restored manifests.
    for (version, sealed, bytes) in [
        (1, &first_sealed, FIRST_BYTES),
        (2, &last_sealed, LAST_BYTES),
    ] {
        let ItemContent::File(doc) = restored.state.items[FILE].versions[(version - 1) as usize]
            .content
            .clone()
        else {
            panic!("restored immutable file manifest is missing");
        };
        let chunk = &doc.chunks[0];
        assert!(restored.media_arrived(
            &crate::media::Wanted {
                plain: chunk.sha256.clone(),
                blob: chunk.blob.clone(),
                key: chunk.key.clone(),
                mime: doc.mime.clone(),
                space: SPACES[0].into(),
                profile: None,
                maximum_sealed_bytes: Some(sealed.len() as u64)
            },
            sealed
        ));
        assert_eq!(
            restored.file_bytes(FILE, Some(version)).unwrap().as_deref(),
            Some(bytes)
        );
    }
    drop(restored);
    let restored = Engine::open(&restored_path).unwrap();
    assert_eq!(
        restored.file_bytes(FILE, Some(1)).unwrap().as_deref(),
        Some(FIRST_BYTES)
    );
    assert_eq!(
        restored.file_bytes(FILE, Some(2)).unwrap().as_deref(),
        Some(LAST_BYTES)
    );
    assert_portable_rows(
        restored.store.conn(),
        &expected_rows,
        "rehydrated and cold reopened",
    );
    drop(restored);
    drop(source);
}
