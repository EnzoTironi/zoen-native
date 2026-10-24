//! Postgres: the directory (identities, handles, devices). Logs and membership live in
//! the log store (`crate::log`).

use protocol::{AgreementKeyRecord, DeviceSigned, KeyPackageRecord, SealedProfile};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use types::{Identity, IdentityKind};

/// Holds the device row for request admission or delivery. Ordinary requests commit
/// before work. Directory mutations keep the fence through their SQL transaction.
/// Only an identity that has not registered yet may act without a device row.
pub async fn authorize_device(
    pool: &PgPool,
    identity: &str,
    device: &str,
    require_device: bool,
) -> Result<Option<Transaction<'static, Postgres>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "SELECT set_config('lock_timeout', '5s', true), set_config('statement_timeout', '5s', true)",
    )
        .execute(&mut *tx)
        .await?;
    let enrolled = sqlx::query_as::<_, (bool, bool)>(
        "SELECT identity = $1, revoked_at IS NOT NULL FROM devices WHERE device = $2 FOR SHARE",
    )
    .bind(identity)
    .bind(device)
    .fetch_optional(&mut *tx)
    .await?;
    // A pending link polls as its own key until it opens its identity box. Enrollment
    // can assign that key to the account before this provisional fetch completes.
    let provisional =
        relay_admission::directory::provisional_allowed(enrolled, identity, device, require_device);
    let unregistered = if provisional {
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM identities WHERE id = $1)")
            .bind(identity)
            .fetch_one(&mut *tx)
            .await?
    } else {
        false
    };
    if relay_admission::directory::request_authorized(enrolled, unregistered) {
        Ok(Some(tx))
    } else {
        tx.rollback().await?;
        Ok(None)
    }
}

pub async fn is_registered(pool: &PgPool, id: &str) -> Result<bool, sqlx::Error> {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('lock_timeout', '1s', true), set_config('statement_timeout', '1s', true)")
            .execute(&mut *tx).await?;
        let known = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM identities WHERE id = $1)")
            .bind(id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(known)
    }).await.map_err(|_| sqlx::Error::PoolTimedOut)?
}

/// Whether a device is enrolled and has not been revoked.
pub async fn device_active(pool: &PgPool, device: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM devices WHERE device = $1 AND revoked_at IS NULL",
    )
    .bind(device)
    .fetch_optional(pool)
    .await?
    .is_some())
}

pub async fn touch_device(
    pool: &PgPool,
    identity: &str,
    device: &str,
    cert: &str,
) -> Result<bool, sqlx::Error> {
    let changed = sqlx::query(
        "UPDATE devices SET last_seen = now()
         WHERE device = $1 AND identity = $2 AND cert = $3 AND revoked_at IS NULL",
    )
    .bind(device)
    .bind(identity)
    .bind(cert)
    .execute(pool)
    .await?;
    Ok(changed.rows_affected() == 1)
}

#[derive(Debug)]
pub enum EnrollmentError {
    InvalidCertificate,
    Conflict,
    Database(sqlx::Error),
}

impl std::fmt::Display for EnrollmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidCertificate => "device certificate invalid",
            Self::Conflict => "device is revoked or belongs to another account",
            Self::Database(_) => "database unavailable",
        })
    }
}

impl std::error::Error for EnrollmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(e) => Some(e),
            _ => None,
        }
    }
}

/// Enrolls a root-certified device after the caller has authorized either an active
/// sponsor or backup recovery in this transaction. A certificate alone is insufficient.
/// A retry may name the same active device; a revoked or conflicting row is never revived.
pub async fn enroll_device(
    tx: &mut Transaction<'_, Postgres>,
    identity: &str,
    device: &str,
    cert: &str,
) -> Result<(), EnrollmentError> {
    if !relay_admission::directory::enrollment_certificate_valid(identity, device, cert) {
        return Err(EnrollmentError::InvalidCertificate);
    }
    let changed = sqlx::query(
        "INSERT INTO devices (device, identity, cert) VALUES ($1, $2, $3)
         ON CONFLICT (device) DO NOTHING",
    )
    .bind(device)
    .bind(identity)
    .bind(cert)
    .execute(&mut **tx)
    .await
    .map_err(EnrollmentError::Database)?;
    if changed.rows_affected() == 0 {
        // Recovery already holds the vault row. An ordinary backup write holds the
        // device FOR SHARE before acquiring that vault row, so a retry must share
        // this existing row too rather than update it and reverse the lock order.
        let active = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM devices WHERE device = $1 AND identity = $2 AND cert = $3
               AND revoked_at IS NULL FOR SHARE",
        )
        .bind(device)
        .bind(identity)
        .bind(cert)
        .fetch_optional(&mut **tx)
        .await
        .map_err(EnrollmentError::Database)?;
        if active.is_none() {
            return Err(EnrollmentError::Conflict);
        }
    }
    Ok(())
}

/// First registration creates the identity and its first device atomically. Existing
/// profile updates require an enrolled active device, held until the update commits.
pub async fn register(
    tx: &mut Transaction<'_, Postgres>,
    profile: &Identity,
    handle: &str,
    device: &str,
    cert: &str,
    first: bool,
) -> Result<(), String> {
    if !event_log::agent_owner::profile_authorized(profile) {
        return Err("profile ownership authorization invalid".into());
    }
    let kind = match profile.kind {
        IdentityKind::Person => "Person",
        IdentityKind::Agent => "Agent",
    };
    let json = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    if first {
        authorize_agent_owner(tx, profile).await?;
        let created = sqlx::query(
            "INSERT INTO identities (id, handle, kind, owner, profile) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&profile.id)
        .bind(handle)
        .bind(kind)
        .bind(&profile.owner)
        .bind(json)
        .execute(&mut **tx)
        .await
        .map_err(directory_error)?;
        if created.rows_affected() != 1 {
            return Err("account already registered; link this device first".into());
        }
        enroll_device(tx, &profile.id, device, cert)
            .await
            .map_err(|e| e.to_string())?;
        if profile.kind == IdentityKind::Agent {
            // The agent's first device is its runtime: it becomes a citizen the relay
            // can account for while the owner is offline (migration 0028).
            crate::agents::claim_citizen(
                tx,
                &crate::agents::CitizenClaim {
                    agent: profile.id.clone(),
                    device: device.to_string(),
                    avatar_style: "hand-drawn-animated".into(),
                    avatar_glyph: profile.glyph.clone(),
                    avatar_tint: Some(profile.tint_hex.clone()),
                    avatar_asset: None,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    } else {
        let active = sqlx::query_scalar::<_, bool>(
            "SELECT identity = $1 AND revoked_at IS NULL FROM devices WHERE device = $2 FOR SHARE",
        )
        .bind(&profile.id)
        .bind(device)
        .fetch_optional(&mut **tx)
        .await
        .map_err(directory_error)?;
        if active != Some(true) {
            return Err("link this device first".into());
        }
        let Some((stored_kind, stored_owner, stored_json)) =
            sqlx::query_as::<_, (String, Option<String>, serde_json::Value)>(
                "SELECT kind, owner, profile FROM identities WHERE id = $1 FOR UPDATE",
            )
            .bind(&profile.id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(directory_error)?
        else {
            return Err("account isn't registered".into());
        };
        if stored_kind != kind || stored_owner != profile.owner {
            return Err("identity kind and owner cannot change".into());
        }
        let stored: Identity = serde_json::from_value(stored_json)
            .map_err(|_| "stored profile invalid".to_string())?;
        if stored.owner_proof != profile.owner_proof {
            // A legacy agent may add its first proof without changing its SQL owner.
            // Already authorized agents retain the original proof after device revocation.
            if stored.owner_proof.is_some() || profile.kind != IdentityKind::Agent {
                return Err("agent ownership proof cannot change".into());
            }
            authorize_agent_owner(tx, profile).await?;
        }
        sqlx::query(
            "UPDATE identities SET handle = $2, profile = $3, updated_at = now() WHERE id = $1",
        )
        .bind(&profile.id)
        .bind(handle)
        .bind(json)
        .execute(&mut **tx)
        .await
        .map_err(directory_error)?;
    }
    Ok(())
}

async fn authorize_agent_owner(
    tx: &mut Transaction<'_, Postgres>,
    profile: &Identity,
) -> Result<(), String> {
    if profile.kind != IdentityKind::Agent {
        return Ok(());
    }
    let (Some(owner), Some(proof)) = (&profile.owner, &profile.owner_proof) else {
        return Err("an agent needs owner authorization".into());
    };
    let person = sqlx::query_scalar::<_, bool>(
        "SELECT kind = 'Person' AND owner IS NULL FROM identities WHERE id = $1 FOR SHARE",
    )
    .bind(owner)
    .fetch_optional(&mut **tx)
    .await
    .map_err(directory_error)?;
    if person != Some(true) {
        return Err("an agent owner must be a registered person".into());
    }
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT identity = $1 AND cert = $3 AND revoked_at IS NULL
         FROM devices WHERE device = $2 FOR SHARE",
    )
    .bind(owner)
    .bind(&proof.device)
    .bind(&proof.cert)
    .fetch_optional(&mut **tx)
    .await
    .map_err(directory_error)?;
    if active != Some(true) {
        return Err("agent authorization device is not enrolled or is revoked".into());
    }
    Ok(())
}

fn directory_error(e: sqlx::Error) -> String {
    match &e {
        sqlx::Error::Database(e) if e.constraint() == Some("identities_handle_key") => {
            "handle_taken".into()
        }
        sqlx::Error::Database(e) if e.constraint() == Some("identities_owner_fkey") => {
            "agent owner isn't registered".into()
        }
        _ => {
            tracing::error!(error = %e, "directory write failed");
            "database unavailable".into()
        }
    }
}

pub async fn lookup(
    pool: &PgPool,
    handle: &str,
    prefix: bool,
) -> Result<Vec<Identity>, sqlx::Error> {
    let rows: Vec<DirectoryRow> = if prefix {
        let pattern = relay_admission::directory::reads::prefix_pattern(handle);
        sqlx::query_as(
            "SELECT id, kind, owner, profile FROM identities WHERE handle LIKE $1 ORDER BY handle LIMIT 20",
        )
        .bind(pattern)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as("SELECT id, kind, owner, profile FROM identities WHERE handle = $1")
            .bind(handle)
            .fetch_all(pool)
            .await?
    };
    Ok(rows
        .into_iter()
        .filter_map(authorized_directory_profile)
        .collect())
}

pub async fn profiles(pool: &PgPool, ids: &[String]) -> Result<Vec<Identity>, sqlx::Error> {
    let rows: Vec<DirectoryRow> =
        sqlx::query_as("SELECT id, kind, owner, profile FROM identities WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(authorized_directory_profile)
        .collect())
}

type DirectoryRow = (String, String, Option<String>, serde_json::Value);

fn authorized_directory_profile((id, kind, owner, json): DirectoryRow) -> Option<Identity> {
    relay_admission::directory::reads::authorized_profile(&id, &kind, owner.as_deref(), json)
}

mod key_material;
mod watermarks;
pub(crate) use key_material::claim_key_packages_on;
pub use key_material::{
    agreement_keys, claim_key_packages, key_package_stock, put_agreement_key, put_key_packages,
    put_key_packages_in, put_profile, sealed_profiles,
};
pub use watermarks::{raise_receipts, receipt_ceilings, receipt_page, ReceiptRow};

// ── Linking devices (ADR 0045) ──

/// Largest sealed link box (identity keys, a certificate, a history manifest).
pub const MAX_LINK_BOX: usize = 64 * 1024;

pub async fn device_revoked(pool: &PgPool, device: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM devices WHERE device = $1 AND revoked_at IS NOT NULL",
    )
    .bind(device)
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// Publishes one immutable pairing box or acknowledges an exact original replay.
/// A live id cannot be overwritten; replay never refreshes publication expiry.
pub async fn put_link_box(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    sealed: &[u8],
    publisher_identity: &str,
    publisher_device: &str,
    target: Option<&protocol::DeviceCertificate>,
) -> Result<bool, sqlx::Error> {
    // Bound maintenance work per publication, and skip cleanup rows held by other
    // publishers. Include this id separately so an expired box can always be reused
    // even when it lies beyond the oldest cleanup batch.
    sqlx::query(
        "WITH expired AS (
             SELECT id FROM link_boxes
             WHERE created_at < now() - interval '15 minutes'
             ORDER BY created_at LIMIT 128 FOR UPDATE SKIP LOCKED
         ), candidates AS (
             SELECT id FROM expired
             UNION
             SELECT id FROM link_boxes
             WHERE id = $1 AND created_at < now() - interval '15 minutes'
         )
         DELETE FROM link_boxes b USING candidates c WHERE b.id = c.id",
    )
    .bind(id)
    .execute(&mut **tx)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO link_boxes (id, sealed, publisher_identity, publisher_device, target_device, target_certificate)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(sealed)
    .bind(publisher_identity)
    .bind(publisher_device)
    .bind(target.map(|value| value.device.as_str()))
    .bind(target.map(|value| value.cert.as_str()))
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(true);
    }
    // Caller still holds active sponsor admission and has rerun any certified
    // target enrollment in this transaction. Equality alone cannot turn an old
    // history/legacy box into enrollment, nor revive a revoked directory row.
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM link_boxes WHERE id = $1 AND sealed = $2
         AND publisher_identity = $3 AND publisher_device = $4
         AND target_device IS NOT DISTINCT FROM $5
         AND target_certificate IS NOT DISTINCT FROM $6
         AND created_at > now() - interval '15 minutes' FOR SHARE",
    )
    .bind(id)
    .bind(sealed)
    .bind(publisher_identity)
    .bind(publisher_device)
    .bind(target.map(|value| value.device.as_str()))
    .bind(target.map(|value| value.cert.as_str()))
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

/// Reads the original sealed bytes until expiry. A lost response must not consume
/// the recipient's only copy. Reads never extend TTL or admit another publication.
pub async fn read_link_box(pool: &PgPool, id: &str) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT sealed FROM link_boxes WHERE id = $1
         AND created_at > now() - interval '15 minutes'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn devices_of(
    pool: &PgPool,
    identity: &str,
) -> Result<Vec<protocol::DeviceRecord>, sqlx::Error> {
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT device, revoked_at IS NOT NULL FROM devices WHERE identity = $1 ORDER BY created_at",
    )
    .bind(identity)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(device, revoked)| protocol::DeviceRecord { device, revoked })
        .collect())
}

/// Whether `device` is a device of `identity` still linked.
pub async fn device_linked(
    pool: &PgPool,
    identity: &str,
    device: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM devices WHERE identity = $1 AND device = $2 AND revoked_at IS NULL",
    )
    .bind(identity)
    .bind(device)
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// Unlinks one of `identity`'s devices and drops its key packages. `false` if it isn't
/// theirs (or already unlinked).
pub async fn revoke_device(
    tx: &mut Transaction<'_, Postgres>,
    identity: &str,
    device: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE devices SET revoked_at = now() WHERE identity = $1 AND device = $2 AND revoked_at IS NULL",
    )
    .bind(identity)
    .bind(device)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM key_packages WHERE identity = $1 AND device = $2")
        .bind(identity)
        .bind(device)
        .execute(&mut **tx)
        .await?;
    Ok(r.rows_affected() == 1)
}

mod push;
pub use push::{
    drop_push_token, drop_voip_push_token, push_targets, set_push_token, voip_push_targets,
};
