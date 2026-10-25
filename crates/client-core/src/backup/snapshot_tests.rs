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
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("backup-canonical")?);
        std::fs::create_dir_all(&path).unwrap();
        Ok(Self(path))
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

mod fixture;
use fixture::{canonical, file, push};

#[path = "snapshot_tests/canonical.rs"]
mod canonical;
