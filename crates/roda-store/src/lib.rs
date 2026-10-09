//! # roda-store
//!
//! Armazenamento local em SQLite (rusqlite com SQLite embutido).
//! Guarda **só** o que é fonte da verdade: Identidades locais e eventos. Todo o resto
//! (lista de Espaços, timeline, Itens, pedidos) é projeção reconstruída do log.
//!
//! Seams para depois: SQLCipher (mesma API, `PRAGMA key`), FTS5 para busca local,
//! App Group compartilhado com as extensões do iOS.

mod profiles;
mod sync;
pub use profiles::ProfileKeyRow;
pub use sync::{OutboxHead, Pending};

use roda_types::{Event, Identity};
use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
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
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

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

    // ── media ──

    pub fn put_media(&self, sha256: &str, mime: &str, bytes: &[u8], at_ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media (sha256, mime, bytes, at_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(sha256) DO NOTHING",
            params![sha256, mime, bytes, at_ms],
        )?;
        Ok(())
    }

    /// Whether these bytes are on this device (without reading them).
    pub fn has_media(&self, sha256: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM media WHERE sha256 = ?1",
                [sha256],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn media(&self, sha256: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT mime, bytes FROM media WHERE sha256 = ?1",
                [sha256],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    // ── search index ──

    pub fn index_clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM search", [])?;
        Ok(())
    }

    /// Replaces the row for (kind, ref_id).
    #[allow(clippy::too_many_arguments)]
    pub fn index_put(
        &self,
        kind: &str,
        ref_id: &str,
        space: &str,
        author: &str,
        at_ms: i64,
        title: &str,
        body: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM search WHERE kind = ?1 AND ref_id = ?2",
            params![kind, ref_id],
        )?;
        self.conn.execute(
            "INSERT INTO search (kind, ref_id, space, author, at_ms, title, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![kind, ref_id, space, author, at_ms, title, body],
        )?;
        Ok(())
    }

    pub fn index_begin(&self) -> Result<()> {
        self.conn.execute_batch("SAVEPOINT idx")?;
        Ok(())
    }

    pub fn index_commit(&self) -> Result<()> {
        self.conn.execute_batch("RELEASE idx")?;
        Ok(())
    }

    /// Undo a failed rebuild. Never leave the savepoint open: every later write would sit
    /// in an uncommitted transaction and vanish when the app is killed.
    pub fn index_abort(&self) {
        let _ = self.conn.execute_batch("ROLLBACK TO idx; RELEASE idx");
    }

    /// Prefix search over the index. `query` is the user's text; every word must match
    /// (as a prefix, diacritics folded). Best matches first, then newest.
    pub fn search(&self, query: &str, kind: &str, limit: u32) -> Result<Vec<SearchRow>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let fts = terms.join(" ");
        let mut st = self.conn.prepare_cached(
            "SELECT kind, ref_id, space, author, at_ms, title,
                    snippet(search, 6, '[[', ']]', '…', 10), snippet(search, 5, '[[', ']]', '…', 8)
             FROM search WHERE search MATCH ?1 AND kind = ?2
             ORDER BY bm25(search, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 1.0), CAST(at_ms AS INTEGER) DESC LIMIT ?3",
        )?;
        let rows = st.query_map(params![fts, kind, limit], |r| {
            Ok(SearchRow {
                kind: r.get(0)?,
                ref_id: r.get(1)?,
                space: r.get(2)?,
                author: r.get(3)?,
                at_ms: r.get::<_, i64>(4)?,
                title: r.get(5)?,
                snippet: r.get(6)?,
                title_snippet: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn index_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM search", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

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
    pub fn append_event(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (space, seq, hash, at_ms, json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                e.space,
                e.seq as i64,
                e.hash,
                e.at_ms,
                serde_json::to_string(e)?
            ],
        )?;
        Ok(())
    }

    /// Replaces the event at (space, seq): a sealed entry this device couldn't open, by
    /// its opened form from another device of the same person (ADR 0045).
    pub fn replace_event(&self, e: &Event) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE events SET at_ms = ?3, json = ?4 WHERE space = ?1 AND seq = ?2",
            params![e.space, e.seq as i64, e.at_ms, serde_json::to_string(e)?],
        )?;
        Ok(n == 1)
    }

    /// Grava vários eventos numa transação (tudo ou nada).
    pub fn append_events(&mut self, events: &[Event]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for e in events {
            tx.execute(
                "INSERT INTO events (space, seq, hash, at_ms, json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    e.space,
                    e.seq as i64,
                    e.hash,
                    e.at_ms,
                    serde_json::to_string(e)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// IDs dos Espaços, na ordem em que nasceram.
    pub fn space_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT space FROM events WHERE seq = 0 ORDER BY at_ms, space")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

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

    pub fn event_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Apaga tudo (usado por "Recomeçar demo").
    pub fn wipe(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM events; DELETE FROM identities; DELETE FROM meta; DELETE FROM search; DELETE FROM media;")?;
        Ok(())
    }

    /// Só para testes de integridade: simula alguém mexendo no arquivo por fora.
    #[doc(hidden)]
    pub fn raw_execute(&self, sql: &str) -> Result<usize> {
        Ok(self.conn.execute(sql, [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roda_types::{EventBody, IdentityKind, Privacy, SpaceKind};

    fn ev(space: &str, seq: u64, hash: &str) -> Event {
        Event {
            space: space.into(),
            seq,
            prev: "p".into(),
            author: "a".into(),
            at_ms: seq as i64,
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
            content: vec![seq as u8],
            sealed_wire: None,
        }
    }

    #[test]
    fn one_process_per_file() {
        let dir = std::env::temp_dir().join(format!("roda-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roda.sqlite");
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
        assert!(s.identities().unwrap().is_empty());
    }
}
