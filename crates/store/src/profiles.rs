//! Encrypted profiles (ADR 0016): the profile keys this device holds (its own and the ones
//! contacts shared), the decrypted fields they opened, contacts' agreement keys and the
//! local blocklist. Same protection class as message content.

use rusqlite::{params, OptionalExtension};

use crate::{Result, Store};

/// A profile key and what it last opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileKeyRow {
    pub identity: String,
    /// 32 bytes, hex.
    pub key: String,
    /// The first profile version this key opens.
    pub key_version: u64,
    /// The newest profile version opened (0 = none yet).
    pub seen_version: u64,
    /// The opened fields (protobuf `ProfileFields`).
    pub fields: Option<Vec<u8>>,
}

impl Store {
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub(crate) fn migrate_profiles(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS profile_keys (
                identity     TEXT PRIMARY KEY,
                key          TEXT NOT NULL,
                key_version  INTEGER NOT NULL,
                seen_version INTEGER NOT NULL DEFAULT 0,
                fields       BLOB
            );
            CREATE TABLE IF NOT EXISTS peer_agreement_keys (
                identity TEXT PRIMARY KEY,
                public   TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS blocked (
                identity TEXT PRIMARY KEY,
                at_ms    INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Reads the profile key and last-opened version for one identity.
    ///
    /// # Errors
    /// Returns SQLite errors, including stored versions outside the unsigned range.
    pub fn profile_key(&self, identity: &str) -> Result<Option<ProfileKeyRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT identity, key, key_version, seen_version, fields FROM profile_keys WHERE identity = ?1",
                [identity],
                row,
            )
            .optional()?)
    }

    /// Reads all retained profile-key records.
    ///
    /// # Errors
    /// Returns SQLite errors, including stored versions outside the unsigned range.
    pub fn profile_keys(&self) -> Result<Vec<ProfileKeyRow>> {
        let mut st = self.conn.prepare_cached(
            "SELECT identity, key, key_version, seen_version, fields FROM profile_keys",
        )?;
        let rows = st.query_map([], row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Stores a key unless one for a newer profile version is already here. Returns whether
    /// it changed anything. Opened fields are kept: they stay true until a newer version opens.
    ///
    /// # Errors
    /// Returns SQLite errors, including a version outside its signed INTEGER range; the key is not replaced on failure.
    pub fn put_profile_key(&self, identity: &str, key: &str, key_version: u64) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT INTO profile_keys (identity, key, key_version) VALUES (?1, ?2, ?3)
             ON CONFLICT(identity) DO UPDATE SET key = excluded.key, key_version = excluded.key_version
             WHERE excluded.key_version > profile_keys.key_version
                OR (excluded.key_version = profile_keys.key_version AND excluded.key <> profile_keys.key)",
            params![identity, key, key_version],
        )?;
        Ok(n > 0)
    }

    /// Records decrypted profile fields and their opened version.
    ///
    /// # Errors
    /// Returns SQLite errors, including a version outside its signed INTEGER range.
    pub fn set_profile_fields(&self, identity: &str, version: u64, fields: &[u8]) -> Result<()> {
        self.conn.execute(
            "UPDATE profile_keys SET seen_version = ?2, fields = ?3 WHERE identity = ?1",
            params![identity, version, fields],
        )?;
        Ok(())
    }

    /// Reads the peer agreement public key, if known.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn peer_agreement_key(&self, identity: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT public FROM peer_agreement_keys WHERE identity = ?1",
                [identity],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Upserts a peer agreement public key.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn put_peer_agreement_key(&self, identity: &str, public: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO peer_agreement_keys (identity, public) VALUES (?1, ?2)
             ON CONFLICT(identity) DO UPDATE SET public = excluded.public",
            params![identity, public],
        )?;
        Ok(())
    }

    /// Blocks an identity once, preserving the original block timestamp.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn block(&self, identity: &str, at_ms: i64) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT INTO blocked (identity, at_ms) VALUES (?1, ?2) ON CONFLICT(identity) DO NOTHING",
            params![identity, at_ms],
        )? > 0)
    }

    /// Removes an identity from the local blocklist.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn unblock(&self, identity: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM blocked WHERE identity = ?1", [identity])?
            > 0)
    }

    /// Lists blocked identities in block timestamp order.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn blocked(&self) -> Result<Vec<String>> {
        let mut st = self
            .conn
            .prepare_cached("SELECT identity FROM blocked ORDER BY at_ms")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Deletes retained profile keys, agreement keys and the local blocklist.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn wipe_profiles(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM profile_keys; DELETE FROM peer_agreement_keys; DELETE FROM blocked;",
        )?;
        Ok(())
    }
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<ProfileKeyRow> {
    Ok(ProfileKeyRow {
        identity: r.get(0)?,
        key: r.get(1)?,
        key_version: r.get::<_, u64>(2)?,
        seen_version: r.get::<_, u64>(3)?,
        fields: r.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn newer_keys_win_and_fields_survive() {
        let s = Store::in_memory().unwrap();
        assert!(s.put_profile_key("ana", "k1", 1).unwrap());
        s.set_profile_fields("ana", 3, b"v3").unwrap();
        assert!(!s.put_profile_key("ana", "k0", 0).unwrap());
        assert!(!s.put_profile_key("ana", "k1", 1).unwrap());
        assert!(s.put_profile_key("ana", "k2", 4).unwrap());
        let r = s.profile_key("ana").unwrap().unwrap();
        assert_eq!(
            (r.key.as_str(), r.key_version, r.seen_version),
            ("k2", 4, 3)
        );
        assert_eq!(r.fields.as_deref(), Some(&b"v3"[..]));
        assert!(s.block("bruno", 1).unwrap());
        assert!(!s.block("bruno", 2).unwrap());
        assert_eq!(s.blocked().unwrap(), vec!["bruno".to_string()]);
        assert!(s.unblock("bruno").unwrap());
        s.wipe_profiles().unwrap();
        assert_eq!(s.profile_keys().unwrap().len(), 0);
    }
}
