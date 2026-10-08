//! Postgres: the directory (identities, handles, devices). Logs and membership live in
//! the log store (`crate::log`).

use roda_proto::{AgreementKeyRecord, DeviceSigned, SealedProfile};
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
