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

#[path = "snapshot_tests/canonical.rs"]
mod canonical;
