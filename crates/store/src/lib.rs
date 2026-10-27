//! # store
//!
//! Armazenamento local em SQLite (rusqlite com SQLite embutido).
//! Guarda **só** o que é fonte da verdade: Identidades locais e eventos. Todo o resto
//! (lista de Espaços, timeline, Itens, pedidos) é projeção reconstruída do log.
//!
//! Seams para depois: `SQLCipher` (mesma API, `PRAGMA key`), FTS5 para busca local,
//! App Group compartilhado com as extensões do iOS.

mod image;
mod media;
mod profiles;
mod recent_attachments;
mod search;
pub use recent_attachments::RecentAttachmentIndexRow;
#[cfg(test)]
mod search_tests;
mod sync;
pub use image::MAX_MEMORY_IMAGE_BYTES;
pub use profiles::ProfileKeyRow;
pub use sync::{OutboxHead, PeerReceipt, Pending};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use types::{Event, Identity};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported or oversized memory database image")]
    InvalidImage,
    #[error("cached media exceeds the requested byte limit")]
    MediaTooLarge,
    #[error("search page limit must be between 1 and 100")]
    InvalidSearchLimit,
}

pub type Result<T> = std::result::Result<T, StoreError>;

const SCHEMA_VERSION: i64 = 2;

pub struct Store {
    conn: Connection,
}

/// One search hit. `snippet` / `title_snippet` mark matches with `[[` `]]`.
#[derive(Debug, Clone)]
pub struct SearchRow {
    pub kind: String,
    pub ref_id: String,
    pub space: String,
    pub author: String,
    pub at_ms: i64,
    pub title: String,
    pub snippet: String,
    pub title_snippet: String,
}

impl Store {
    /// Opens and migrates one exclusively owned file-backed store.
    ///
    /// # Errors
    /// Returns SQLite errors when opening, acquiring the exclusive lock or migrating the database.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(0))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Um processo por banco: com locking_mode=EXCLUSIVE, a primeira escrita
        // segura o lock até fechar. Um segundo app no mesmo arquivo falha aqui em
        // vez de escrever com uma projeção desatualizada.
        conn.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
        conn.execute_batch("BEGIN IMMEDIATE; COMMIT;")?;
        let s = Self { conn };
        s.migrate()?;
        s.migrate_sync()?;
        s.migrate_profiles()?;
        Ok(s)
    }

    /// The device database, for state that commits with the log (MLS groups, ADR 0026).
    #[must_use]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Mutable access to the same connection, preserving the store ownership boundary.
    #[must_use]
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Creates and migrates a private in-memory store.
    ///
    /// # Errors
    /// Returns SQLite errors when allocating or migrating the in-memory database.
    pub fn in_memory() -> Result<Self> {
        let s = Self {
            conn: Connection::open_in_memory()?,
        };
        s.migrate()?;
        s.migrate_sync()?;
        s.migrate_profiles()?;
        Ok(s)
    }

    fn migrate(&self) -> Result<()> {
        let v: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if v < 1 {
            self.conn.execute_batch(
                "
                CREATE TABLE identities (
                    id      TEXT PRIMARY KEY,
                    profile TEXT NOT NULL,
                    -- Segredo Ed25519 só para Identidades que vivem neste aparelho.
                    -- No app de verdade: cifrado pela passkey (PRF) / Secure Enclave.
                    secret  BLOB
                );
                CREATE TABLE events (
                    space   TEXT    NOT NULL,
                    seq     INTEGER NOT NULL,
                    hash    TEXT    NOT NULL UNIQUE,
                    at_ms   INTEGER NOT NULL,
                    json    TEXT    NOT NULL,
                    PRIMARY KEY (space, seq)
                ) WITHOUT ROWID;
                CREATE TABLE meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )?;
            self.conn.pragma_update(None, "user_version", 1)?;
        }
        if v < 2 {
            self.conn.execute_batch(
                "
                -- Content-addressed attachments (chat background photos for now).
                CREATE TABLE IF NOT EXISTS media (
                    sha256 TEXT PRIMARY KEY,
                    mime   TEXT NOT NULL,
                    bytes  BLOB NOT NULL,
                    at_ms  INTEGER NOT NULL
                );
                -- Local search index (a projection of the log, rebuilt at will). Diacritics
                -- are folded, so 'acao' finds 'ação'.
                CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(
                    kind UNINDEXED, ref_id UNINDEXED, space UNINDEXED, author UNINDEXED, at_ms UNINDEXED,
                    title, body,
                    tokenize = 'unicode61 remove_diacritics 2'
                );
                ",
            )?;
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    /// Upserts an identity profile while retaining any previously stored local secret.
    ///
    /// # Errors
    /// Returns JSON encoding or SQLite errors while storing the profile and optional local secret.
    pub fn put_identity(&self, identity: &Identity, secret: Option<&[u8; 32]>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO identities (id, profile, secret) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET profile = excluded.profile",
            params![
                identity.id,
                serde_json::to_string(identity)?,
                secret.map(|s| s.to_vec())
            ],
        )?;
        Ok(())
    }

    /// Todas as Identidades conhecidas, com o segredo quando ele vive aqui.
    ///
    /// # Errors
    /// Returns SQLite errors or JSON decoding errors in a stored identity profile.
    pub fn identities(&self) -> Result<Vec<(Identity, Option<[u8; 32]>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT profile, secret FROM identities ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (profile, secret) = row?;
            let identity: Identity = serde_json::from_str(&profile)?;
            let secret = secret.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
            out.push((identity, secret));
        }
        Ok(out)
    }

    /// Append-only: um `(space, seq)` repetido falha (a PRIMARY KEY garante).
    ///
    /// # Errors
    /// Returns JSON encoding or SQLite insertion errors, including duplicate keys and sequences outside SQLite INTEGER.
    pub fn append_event(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (space, seq, hash, at_ms, json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![e.space, e.seq, e.hash, e.at_ms, serde_json::to_string(e)?],
        )?;
        Ok(())
    }

    /// Replaces the event at (space, seq): a sealed entry this device couldn't open, by
    /// its opened form from another device of the same person (ADR 0045).
    ///
    /// # Errors
    /// Returns JSON encoding or SQLite update errors, including sequences outside SQLite INTEGER.
    pub fn replace_event(&self, e: &Event) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE events SET at_ms = ?3, json = ?4 WHERE space = ?1 AND seq = ?2",
            params![e.space, e.seq, e.at_ms, serde_json::to_string(e)?],
        )?;
        Ok(n == 1)
    }

    /// Grava vários eventos numa transação (tudo ou nada).
    ///
    /// # Errors
    /// Returns JSON encoding or SQLite errors; the transaction rolls back the entire batch on failure.
    pub fn append_events(&mut self, events: &[Event]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for e in events {
            tx.execute(
                "INSERT INTO events (space, seq, hash, at_ms, json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.space, e.seq, e.hash, e.at_ms, serde_json::to_string(e)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// IDs dos Espaços, na ordem em que nasceram.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn space_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT space FROM events WHERE seq = 0 ORDER BY at_ms, space")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Reads retained events for one space in canonical sequence order.
    ///
    /// # Errors
    /// Returns SQLite errors or JSON decoding errors in a retained event.
    pub fn events(&self, space: &str) -> Result<Vec<Event>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM events WHERE space = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([space], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            out.push(serde_json::from_str(&json?)?);
        }
        Ok(out)
    }

    /// Counts the retained canonical events.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn event_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, u64>(0))?)
    }

    /// Reads one device-local metadata value, or `None` when absent.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    /// Upserts one device-local metadata value.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Apaga tudo (usado por "Recomeçar demo").
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn wipe(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM events; DELETE FROM identities; DELETE FROM meta; DELETE FROM search; DELETE FROM media; DELETE FROM peer_receipts;")?;
        Ok(())
    }

    /// Só para testes de integridade: simula alguém mexendo no arquivo por fora.
    #[doc(hidden)]
    ///
    /// # Errors
    /// Returns the SQLite error from the supplied fixture SQL.
    pub fn raw_execute(&self, sql: &str) -> Result<usize> {
        Ok(self.conn.execute(sql, [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{EventBody, IdentityKind, Privacy, SpaceKind};

    fn ev(space: &str, seq: u8, hash: &str) -> Event {
        Event {
            space: space.into(),
            seq: u64::from(seq),
            prev: "p".into(),
            author: "a".into(),
            at_ms: i64::from(seq),
            body: EventBody::SpaceCreated {
                title: "t".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
            hash: hash.into(),
            sig: "s".into(),
            client_id: format!("c{seq}"),
            device: None,
            cert: None,
            seen: None,
            content: vec![seq],
            sealed_wire: None,
        }
    }

    #[test]
    fn one_process_per_file() {
        let dir = std::env::temp_dir().join(format!("store-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("client.sqlite");
        let path = path.to_str().unwrap();
        let first = Store::open(path).unwrap();
        assert!(
            Store::open(path).is_err(),
            "segundo processo no mesmo banco é recusado"
        );
        drop(first);
        assert!(Store::open(path).is_ok(), "depois de fechar, abre de novo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn events_roundtrip_in_order_and_are_append_only() {
        let s = Store::in_memory().unwrap();
        s.append_event(&ev("sp_a", 0, "h0")).unwrap();
        s.append_event(&ev("sp_a", 1, "h1")).unwrap();
        s.append_event(&ev("sp_b", 0, "h2")).unwrap();
        assert!(
            s.append_event(&ev("sp_a", 1, "h9")).is_err(),
            "não reescreve seq existente"
        );
        let a = s.events("sp_a").unwrap();
        assert_eq!(a.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(s.space_ids().unwrap().len(), 2);
        assert_eq!(s.event_count().unwrap(), 3);
    }

    #[test]
    fn batch_is_atomic() {
        let mut s = Store::in_memory().unwrap();
        s.append_event(&ev("sp_a", 0, "h0")).unwrap();
        let res = s.append_events(&[ev("sp_a", 1, "h1"), ev("sp_a", 0, "dup")]);
        assert!(res.is_err());
        assert_eq!(s.events("sp_a").unwrap().len(), 1, "rollback total");
    }

    #[test]
    fn identities_and_meta_persist() {
        let s = Store::in_memory().unwrap();
        let id = Identity {
            owner_proof: None,
            id: "abc".into(),
            kind: IdentityKind::Agent,
            name: "Financeiro".into(),
            handle: "financeiro".into(),
            tint_hex: "#34C759".into(),
            glyph: Some("chart.pie.fill".into()),
            owner: Some("enzo".into()),
            bio: String::new(),
        };
        s.put_identity(&id, Some(&[7u8; 32])).unwrap();
        let all = s.identities().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, id);
        assert_eq!(all[0].1, Some([7u8; 32]));
        s.set_meta("me", "abc").unwrap();
        assert_eq!(s.meta("me").unwrap().as_deref(), Some("abc"));
        s.wipe().unwrap();
        assert_eq!(s.identities().unwrap().len(), 0);
    }

    #[test]
    fn numeric_sequence_overflow_preserves_original_and_rolls_back_batch() -> Result<()> {
        let mut store = Store::in_memory()?;
        let original = ev("bounded", 0, "original");
        store.append_event(&original)?;
        let retained: String = store
            .conn
            .query_row("SELECT json FROM events", [], |row| row.get(0))?;
        let mut oversized = ev("bounded", 2, "oversized");
        oversized.seq = u64::MAX;
        assert!(store.append_event(&oversized).is_err());
        assert!(store.replace_event(&oversized).is_err());
        assert!(store
            .append_events(&[ev("bounded", 1, "next"), oversized])
            .is_err());
        assert_eq!(store.events("bounded")?, vec![original]);
        assert_eq!(store.event_count()?, 1);
        assert_eq!(
            store
                .conn
                .query_row("SELECT json FROM events", [], |row| row.get::<_, String>(0))?,
            retained
        );
        Ok(())
    }

    #[test]
    fn numeric_profile_versions_refuse_overflow_and_negative_stored_rows() -> Result<()> {
        let store = Store::in_memory()?;
        store.put_profile_key("owner", "retained-key", 7)?;
        store.set_profile_fields("owner", 8, b"retained fields")?;
        let retained = store.profile_key("owner")?;
        assert!(store
            .put_profile_key("owner", "replacement", u64::MAX)
            .is_err());
        assert!(store
            .set_profile_fields("owner", u64::MAX, b"replacement")
            .is_err());
        assert_eq!(store.profile_key("owner")?, retained);
        store.put_profile_key("max", "max-key", i64::MAX.unsigned_abs())?;
        assert_eq!(
            store.profile_key("max")?.map(|row| row.key_version),
            Some(i64::MAX.unsigned_abs())
        );
        for column in ["key_version", "seen_version"] {
            // Both column names are this fixed test fixture, not untrusted SQL.
            store.conn.execute(
                &format!("UPDATE profile_keys SET {column}=-1 WHERE identity='owner'"),
                [],
            )?;
            assert!(store.profile_key("owner").is_err());
            store.conn.execute(
                &format!("UPDATE profile_keys SET {column}=?1 WHERE identity='owner'"),
                [if column == "key_version" { 7 } else { 8 }],
            )?;
        }
        Ok(())
    }

    #[test]
    fn numeric_retry_counts_refuse_corruption_without_rewriting_retained_bytes() -> Result<()> {
        let store = Store::in_memory()?;
        let original = ev("pending", 0, "original");
        store.outbox_put(&original)?;
        let retained = store.outbox_original_json(&original.client_id)?;
        for attempts in [-1, i64::from(u32::MAX) + 1] {
            store
                .conn
                .execute("UPDATE outbox SET attempts=?1", [attempts])?;
            assert!(store.outbox().is_err());
            assert_eq!(store.outbox_original_json(&original.client_id)?, retained);
        }
        Ok(())
    }
}
