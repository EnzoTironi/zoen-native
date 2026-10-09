//! Postgres: the directory (identities, handles, devices). Logs and membership live in
//! the log store (`crate::log`).

use roda_proto::{AgreementKeyRecord, DeviceSigned, KeyPackageRecord, SealedProfile};
use roda_types::{Identity, IdentityKind};
use sqlx::PgPool;

pub async fn is_registered(pool: &PgPool, id: &str) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT id FROM identities WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .is_some(),
    )
}

pub async fn device_known(pool: &PgPool, device: &str) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT 1 FROM devices WHERE device = $1")
            .bind(device)
            .fetch_optional(pool)
            .await?
            .is_some(),
    )
}

pub async fn touch_device(
    pool: &PgPool,
    identity: &str,
    device: &str,
    cert: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO devices (device, identity, cert) VALUES ($1, $2, $3)
         ON CONFLICT (device) DO UPDATE SET last_seen = now()",
    )
    .bind(device)
    .bind(identity)
    .bind(cert)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn register(pool: &PgPool, profile: &Identity, handle: &str) -> Result<(), String> {
    let kind = match profile.kind {
        IdentityKind::Person => "Person",
        IdentityKind::Agent => "Agent",
    };
    let json = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    let r = sqlx::query(
        "INSERT INTO identities (id, handle, kind, owner, profile) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO UPDATE SET handle = excluded.handle, profile = excluded.profile, updated_at = now()",
    )
    .bind(&profile.id)
    .bind(handle)
    .bind(kind)
    .bind(&profile.owner)
    .bind(json)
    .execute(pool)
    .await;
    match r {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("identities_handle_key") => {
            Err("handle_taken".into())
        }
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("identities_owner_fkey") => {
            Err("agent owner isn't registered".into())
        }
        Err(e) => {
            tracing::error!(error = %e, "register failed");
            Err("database unavailable".into())
        }
    }
}

pub async fn lookup(
    pool: &PgPool,
    handle: &str,
    prefix: bool,
) -> Result<Vec<Identity>, sqlx::Error> {
    let rows: Vec<serde_json::Value> = if prefix {
        let pattern = format!("{}%", handle.replace(['\\', '%'], "").replace('_', "\\_"));
        sqlx::query_scalar(
            "SELECT profile FROM identities WHERE handle LIKE $1 ORDER BY handle LIMIT 20",
        )
        .bind(pattern)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_scalar("SELECT profile FROM identities WHERE handle = $1")
            .bind(handle)
            .fetch_all(pool)
            .await?
    };
    Ok(rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

pub async fn profiles(pool: &PgPool, ids: &[String]) -> Result<Vec<Identity>, sqlx::Error> {
    let rows: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT profile FROM identities WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

pub async fn put_agreement_key(
    pool: &PgPool,
    identity: &str,
    public: &str,
    s: &DeviceSigned,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agreement_keys (identity, public, device, sig, cert) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (identity) DO UPDATE SET public = excluded.public, device = excluded.device,
           sig = excluded.sig, cert = excluded.cert, updated_at = now()",
    )
    .bind(identity)
    .bind(public)
    .bind(&s.device)
    .bind(&s.sig)
    .bind(&s.cert)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn agreement_keys(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<AgreementKeyRecord>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT identity, public, device, sig, cert FROM agreement_keys WHERE identity = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(identity, public, device, sig, cert)| AgreementKeyRecord {
            identity,
            public,
            signed: DeviceSigned { device, sig, cert },
        })
        .collect())
}

/// Stores `p` if its version is newer than the stored one. `false` = stale.
/// Stores a newer version; resending the exact stored bytes is a no-op success.
pub async fn put_profile(pool: &PgPool, p: &SealedProfile) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "INSERT INTO profiles (identity, version, ciphertext, device, sig, cert) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (identity) DO UPDATE SET version = excluded.version, ciphertext = excluded.ciphertext,
           device = excluded.device, sig = excluded.sig, cert = excluded.cert, updated_at = now()
         WHERE profiles.version < excluded.version
            OR (profiles.version = excluded.version AND profiles.ciphertext = excluded.ciphertext)",
    )
    .bind(&p.identity)
    .bind(p.version as i64)
    .bind(&p.ciphertext)
    .bind(&p.signed.device)
    .bind(&p.signed.sig)
    .bind(&p.signed.cert)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn sealed_profiles(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<SealedProfile>, sqlx::Error> {
    let rows: Vec<(String, i64, Vec<u8>, String, String, String)> = sqlx::query_as(
        "SELECT identity, version, ciphertext, device, sig, cert FROM profiles WHERE identity = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(identity, version, ciphertext, device, sig, cert)| SealedProfile {
                identity,
                version: version as u64,
                ciphertext,
                signed: DeviceSigned { device, sig, cert },
            },
        )
        .collect())
}

/// Single-use key packages a device may keep stored; publishing past it is refused.
pub const MAX_KEY_PACKAGES: i64 = 200;

/// Stores a device's key packages, and its last-resort one in place of the previous.
/// `false` when that would keep more than [`MAX_KEY_PACKAGES`] single-use ones.
pub async fn put_key_packages(
    pool: &PgPool,
    identity: &str,
    device: &str,
    packages: &[Vec<u8>],
    last_resort: Option<&[u8]>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // One publisher per device at a time, so the count below holds when we insert.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("kp:{identity}:{device}"))
        .execute(&mut *tx)
        .await?;
    let (stored,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM key_packages WHERE identity = $1 AND device = $2 AND NOT last_resort",
    )
    .bind(identity)
    .bind(device)
    .fetch_one(&mut *tx)
    .await?;
    if stored + packages.len() as i64 > MAX_KEY_PACKAGES {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO key_packages (identity, device, data) SELECT $1, $2, unnest($3::bytea[])",
    )
    .bind(identity)
    .bind(device)
    .bind(packages)
    .execute(&mut *tx)
    .await?;
    if let Some(data) = last_resort {
        sqlx::query(
            "INSERT INTO key_packages (identity, device, data, last_resort) VALUES ($1, $2, $3, true)
             ON CONFLICT (identity, device) WHERE last_resort DO UPDATE SET data = excluded.data, created_at = now()",
        )
        .bind(identity)
        .bind(device)
        .bind(data)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Single-use key packages each device has left, for devices that have published (they
/// hold a last-resort one). A device that never published isn't listed: it publishes a
/// full set on its own.
pub async fn key_package_stock(
    pool: &PgPool,
    devices: &[(String, String)],
) -> Result<Vec<(String, String, i64)>, sqlx::Error> {
    let (ids, devs): (Vec<String>, Vec<String>) = devices.iter().cloned().unzip();
    sqlx::query_as(
        "SELECT d.identity, d.device,
                (SELECT count(*) FROM key_packages k
                 WHERE k.identity = d.identity AND k.device = d.device AND NOT k.last_resort)
         FROM unnest($1::text[], $2::text[]) AS d(identity, device)
         WHERE EXISTS (SELECT 1 FROM key_packages k
                       WHERE k.identity = d.identity AND k.device = d.device AND k.last_resort)",
    )
    .bind(&ids)
    .bind(&devs)
    .fetch_all(pool)
    .await
}

/// One key package per device of each identity: the oldest single-use one, taken (deleted)
/// in the same statement, or else the device's last-resort one, left in place. Concurrent
/// claimers skip each other's rows, so no single-use package is handed out twice.
pub async fn claim_key_packages(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<KeyPackageRecord>, sqlx::Error> {
    let rows: Vec<(String, String, Vec<u8>)> = sqlx::query_as(
        "WITH devices AS (
             SELECT DISTINCT identity, device FROM key_packages WHERE identity = ANY($1)
         ), taken AS (
             DELETE FROM key_packages WHERE id IN (
                 SELECT k.id FROM devices d CROSS JOIN LATERAL (
                     SELECT id FROM key_packages k
                     WHERE k.identity = d.identity AND k.device = d.device AND NOT k.last_resort
                     ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED
                 ) k
             ) RETURNING identity, device, data
         )
         SELECT identity, device, data FROM taken
         UNION ALL
         SELECT k.identity, k.device, k.data FROM key_packages k JOIN devices d USING (identity, device)
         WHERE k.last_resort
           AND NOT EXISTS (SELECT 1 FROM taken t WHERE t.identity = k.identity AND t.device = k.device)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(identity, device, data)| KeyPackageRecord {
            identity,
            device,
            data,
        })
        .collect())
}
