//! SQLite snapshot validation, export and transactional restoration.
use super::{
    invalid, random, storage, t, CoreError, Engine, Header, KEEP_TABLES, PAYLOAD_VERSION, R,
};

fn temp_path(tag: &str) -> R<std::path::PathBuf> {
    Ok(std::env::temp_dir().join(format!("zoen-{tag}-{}.db", hex::encode(random::<8>()?))))
}

/// Removes a temp database and its WAL/SHM siblings.
fn remove_db(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    for ext in ["-wal", "-shm", "-journal"] {
        let mut p = path.as_os_str().to_owned();
        p.push(ext);
        let _ = std::fs::remove_file(p);
    }
}

/// Baseline portable v1 columns. Nullable fields and defaulted cache columns
/// remain optional, so importing a retained snapshot keeps the old intersection
/// compatibility rather than requiring today's complete schema.
const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    ("identities", &["id", "profile"]),
    ("events", &["space", "seq", "hash", "at_ms", "json"]),
    ("meta", &["key", "value"]),
    ("profile_keys", &["identity", "key", "key_version"]),
    ("peer_agreement_keys", &["identity", "public"]),
    ("blocked", &["identity", "at_ms"]),
    ("synced_spaces", &["space"]),
    ("media_keys", &["plain_sha", "blob_sha", "key"]),
];

fn corrupt_snapshot() -> CoreError {
    invalid(t(
        "O backup está corrompido ou é incompatível.",
        "The backup is corrupted or unsupported.",
    ))
}

fn snapshot_root(header: &Header) -> R<event_log::Signer> {
    let secret: [u8; 32] = hex::decode(&header.identity_secret)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(corrupt_snapshot)?;
    let agreement = hex::decode(&header.agreement_secret).map_err(|_| corrupt_snapshot())?;
    let root = event_log::Signer::from_secret(&secret);
    if header.version != PAYLOAD_VERSION || root.id() != header.identity || agreement.len() != 32 {
        return Err(corrupt_snapshot());
    }
    Ok(root)
}

fn snapshot_columns(conn: &rusqlite::Connection, schema: &str, table: &str) -> R<Vec<String>> {
    let mut statement = conn
        .prepare(&format!("PRAGMA {schema}.table_info(\"{table}\")"))
        .map_err(storage)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    Ok(columns)
}

/// Shared import operation, also exercised against a disposable current schema
/// before any enrollment or target wipe. No rows are silently omitted on failure.
fn copy_snapshot_tables(conn: &rusqlite::Connection) -> R<()> {
    for table in KEEP_TABLES {
        let theirs = snapshot_columns(conn, "bk", table)?;
        let shared: Vec<_> = snapshot_columns(conn, "main", table)?
            .into_iter()
            .filter(|column| theirs.contains(column))
            .map(|column| format!("\"{column}\""))
            .collect();
        let list = shared.join(", ");
        conn.execute_batch(&format!(
            "DELETE FROM main.\"{table}\";
            INSERT INTO main.\"{table}\" ({list}) SELECT {list} FROM bk.\"{table}\";"
        ))
        .map_err(storage)?;
    }
    // These encrypted drafts belong to the old device even in retained snapshots.
    conn.execute("DELETE FROM main.meta WHERE key LIKE 'message.draft.%' OR key LIKE 'media.draft.%' OR key LIKE 'file.edit.draft.%' OR key LIKE 'page.draft.%'", []).map_err(storage)?;
    Ok(())
}

/// Authenticated envelope bytes are not proof of a complete SQLite snapshot.
/// Inspect SQLite itself without running Engine migrations on the input, then
/// exercise the real import against an ephemeral current-schema Store. Only the
/// caller's later, separately admitted restore changes its device/account.
pub(crate) fn preflight_snapshot(header: &Header, db: &[u8]) -> R<()> {
    snapshot_root(header)?;
    let path = temp_path("preflight")?;
    let checked = (|| -> R<()> {
        std::fs::write(&path, db).map_err(storage)?;
        let staged = store::Store::in_memory()?;
        let conn = staged.conn();
        conn.pragma_update(None, "trusted_schema", false)
            .map_err(storage)?;
        conn.execute(
            "ATTACH DATABASE ?1 AS bk",
            [path.to_str().ok_or_else(|| storage("temp path"))?],
        )
        .map_err(storage)?;
        let integrity: String = conn
            .query_row("PRAGMA bk.integrity_check(1)", [], |row| row.get(0))
            .map_err(storage)?;
        if integrity != "ok" {
            return Err(corrupt_snapshot());
        }
        let mut table_list = conn.prepare("PRAGMA bk.table_list").map_err(storage)?;
        let tables = table_list
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(storage)?
            .collect::<Result<std::collections::HashMap<_, _>, _>>()
            .map_err(storage)?;
        for (table, required) in REQUIRED_COLUMNS {
            if tables.get(*table).map(String::as_str) != Some("table") {
                return Err(corrupt_snapshot());
            }
            let columns = snapshot_columns(conn, "bk", table)?;
            if required
                .iter()
                .any(|column| !columns.iter().any(|present| present == column))
            {
                return Err(corrupt_snapshot());
            }
        }
        let transaction = conn.unchecked_transaction().map_err(storage)?;
        copy_snapshot_tables(conn)?;
        transaction.commit().map_err(storage)?;
        // SQLite affinity is not strict typing. Exercise the real readers used
        // by reload/unlock before they can fail after enrollment and vault writes.
        staged.identities()?;
        staged.synced_spaces()?;
        let profile_keys = staged.profile_keys()?;
        if profile_keys.iter().any(|row| {
            row.identity == header.identity
                && event_log::profile::ProfileKey::from_hex(&row.key).is_none()
        }) {
            return Err(corrupt_snapshot());
        }
        if staged.meta("me")?.as_deref() != Some(header.identity.as_str())
            || staged.meta(crate::sync::EVENT_FORMAT_META)?.as_deref()
                != Some(crate::sync::EVENT_FORMAT_VALUE)
        {
            return Err(corrupt_snapshot());
        }
        let mut identities = conn
            .prepare("SELECT id, profile FROM main.identities")
            .map_err(storage)?;
        let mut rows = identities.query([]).map_err(storage)?;
        let mut has_root = false;
        while let Some(row) = rows.next().map_err(storage)? {
            let id: String = row.get(0).map_err(storage)?;
            let profile: String = row.get(1).map_err(storage)?;
            let identity: types::Identity = serde_json::from_str(&profile).map_err(storage)?;
            if identity.id != id {
                return Err(corrupt_snapshot());
            }
            has_root |= id == header.identity;
        }
        if !has_root {
            return Err(corrupt_snapshot());
        }
        let mut events = conn
            .prepare("SELECT space, seq, hash, at_ms, json FROM main.events ORDER BY space, seq")
            .map_err(storage)?;
        let mut rows = events.query([]).map_err(storage)?;
        let mut log: Option<event_log::SpaceLog> = None;
        while let Some(row) = rows.next().map_err(storage)? {
            let space: String = row.get(0).map_err(storage)?;
            let seq: i64 = row.get(1).map_err(storage)?;
            let hash: String = row.get(2).map_err(storage)?;
            let at_ms: i64 = row.get(3).map_err(storage)?;
            let json: String = row.get(4).map_err(storage)?;
            let event: types::Event = serde_json::from_str(&json).map_err(storage)?;
            if event.space != space
                || u64::try_from(seq).ok() != Some(event.seq)
                || event.hash != hash
                || event.at_ms != at_ms
            {
                return Err(corrupt_snapshot());
            }
            if log.as_ref().is_none_or(|log| log.space() != space) {
                log = Some(event_log::SpaceLog::new(&space));
            }
            log.as_mut()
                .ok_or_else(corrupt_snapshot)?
                .accept(event)
                .map_err(storage)?;
        }
        Ok(())
    })();
    remove_db(&path);
    // Input/schema/import errors are a refused backup, not an accepted restore
    // followed by a partially installed account. The disposable Store is dropped.
    checked.map_err(|_| corrupt_snapshot())
}

impl Engine {
    /// The device database as a backup sees it: only [`KEEP_TABLES`], without this
    /// device's account, MLS bookkeeping or pending invites.
    pub(crate) fn backup_snapshot(&self) -> R<Vec<u8>> {
        let path = temp_path("backup")?;
        let out = (|| -> R<Vec<u8>> {
            self.store
                .conn()
                .execute(
                    "VACUUM INTO ?1",
                    [path.to_str().ok_or_else(|| storage("temp path"))?],
                )
                .map_err(storage)?;
            let c = rusqlite::Connection::open(&path).map_err(storage)?;
            let names = |sql: &str| -> R<Vec<String>> {
                let mut st = c.prepare(sql).map_err(storage)?;
                let rows = st
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(storage);
                rows
            };
            // Virtual tables (the search index) first: dropping one takes its shadow tables.
            for v in names(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL%'",
            )? {
                c.execute_batch(&format!("DROP TABLE IF EXISTS \"{v}\""))
                    .map_err(storage)?;
            }
            for n in names(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )? {
                if !KEEP_TABLES.contains(&n.as_str()) {
                    c.execute_batch(&format!("DROP TABLE IF EXISTS \"{n}\""))
                        .map_err(storage)?;
                }
            }
            c.execute_batch(
                "DELETE FROM meta WHERE key = 'account' OR key = 'relay_name' OR key LIKE 'backup.%'
                    OR key LIKE 'mls.%' OR key LIKE 'invite:%' OR key LIKE 'message.draft.%'
                    OR key LIKE 'media.draft.%' OR key LIKE 'file.edit.draft.%'
                    OR key LIKE 'page.draft.%';
                 VACUUM;",
            )
            .map_err(storage)?;
            drop(c);
            std::fs::read(&path).map_err(storage)
        })();
        remove_db(&path);
        out
    }

    /// Installs a backup's tables on this (empty) device and makes it the identity's new
    /// device. Returns the new device secret; the caller stores it and the restored
    /// secrets in the vault.
    pub(crate) fn restore_snapshot(
        &mut self,
        header: &Header,
        db: &[u8],
        device: event_log::Signer,
    ) -> R<[u8; 32]> {
        if self.account().is_some() {
            return Err(invalid(t(
                "Este aparelho já tem uma conta.",
                "This device already has an account.",
            )));
        }
        preflight_snapshot(header, db)?;
        let root = snapshot_root(header)?;
        let path = temp_path("restore")?;
        if !self.is_empty() {
            self.wipe()?;
        }
        let res = (|| -> R<()> {
            std::fs::write(&path, db).map_err(storage)?;
            let conn = self.store.conn();
            conn.execute(
                "ATTACH DATABASE ?1 AS bk",
                [path.to_str().ok_or_else(|| storage("temp path"))?],
            )
            .map_err(storage)?;
            let copy = (|| -> R<()> {
                conn.execute_batch("BEGIN").map_err(storage)?;
                copy_snapshot_tables(conn)?;
                conn.execute_batch("COMMIT").map_err(storage)
            })();
            if copy.is_err() {
                let _ = conn.execute_batch("ROLLBACK");
            }
            let _ = conn.execute_batch("DETACH DATABASE bk");
            copy
        })();
        remove_db(&path);
        res?;
        let device_secret = device.secret();
        let device_id = device.id();
        self.install_restored_account(&root, device, &header.relay_url)?;
        // The snapshot has history but no MLS state. Persist requests for this new
        // device so a surviving group admin can add its leaf, even after a relaunch.
        let me = root.id();
        let spaces: Vec<_> = self
            .net
            .synced
            .iter()
            .filter(|space| {
                self.is_e2e(space) && self.mls_roster(space).is_some_and(|r| r.contains_key(&me))
            })
            .cloned()
            .collect();
        for space in spaces {
            self.append(
                &space,
                &me,
                types::EventBody::DeviceJoining {
                    device: device_id.clone(),
                },
            )?;
        }
        Ok(device_secret)
    }
}
