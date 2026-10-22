use super::*;

// ───────────────────────────── restore (no device yet) ─────────────────────────────

struct VaultRow {
    identity: String,
    mode: String,
    oprf_key: Option<Vec<u8>>,
    verifier: Vec<u8>,
    wrapped_key: Vec<u8>,
    kdf: serde_json::Value,
    locked: bool,
    blob_bytes: Option<i64>,
    blob_key: Option<String>,
    generation: Option<String>,
}

async fn vault_by_handle(
    st: &Shared,
    handle: &str,
) -> Result<(Option<VaultRow>, Transaction<'static, Postgres>), Response> {
    let mut tx = begin(st).await?;
    let row = sqlx::query(
        "SELECT v.identity, v.mode, v.oprf_key, v.verifier, v.wrapped_key, v.kdf, v.locked,
                v.blob_bytes, v.blob_key, v.generation
         FROM backup_vaults v JOIN identities i ON i.id = v.identity
         WHERE i.handle = $1 FOR UPDATE OF v",
    )
    .bind(handle)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    Ok((
        row.map(|r| VaultRow {
            identity: r.get("identity"),
            mode: r.get("mode"),
            oprf_key: r.get("oprf_key"),
            verifier: r.get("verifier"),
            wrapped_key: r.get("wrapped_key"),
            kdf: r.get("kdf"),
            locked: r.get("locked"),
            blob_bytes: r.get("blob_bytes"),
            blob_key: r.get("blob_key"),
            generation: r.get("generation"),
        }),
        tx,
    ))
}

/// One more guess; at the limit the OPRF key is destroyed and the vault locked for good.
/// `armed`: the guess was an OPRF evaluation, so the open that follows it is free.
async fn spend_guess(
    conn: &mut PgConnection,
    identity: &str,
    armed: bool,
) -> Result<bool, Response> {
    let locked: bool = sqlx::query_scalar(
        "UPDATE backup_vaults SET guesses = guesses + 1, armed = $3,
             locked = locked OR guesses + 1 >= $2,
             oprf_key = CASE WHEN guesses + 1 >= $2 THEN NULL ELSE oprf_key END
         WHERE identity = $1 RETURNING locked",
    )
    .bind(identity)
    .bind(MAX_GUESSES)
    .bind(armed)
    .fetch_one(conn)
    .await
    .map_err(|_| unavailable())?;
    if locked {
        tracing::warn!(identity = %crate::pseudonym::pseudo(identity), "backup vault locked after too many guesses");
    }
    Ok(locked)
}

fn restore_limit(st: &Shared, ip: &str, handle: &str) -> Result<(), Response> {
    st.limits.backup_restore_ip.check(ip).map_err(slow)?;
    st.limits
        .backup_restore_handle
        .check(handle)
        .map_err(slow)?;
    Ok(())
}

fn default_kdf() -> serde_json::Value {
    serde_json::json!({"alg": "argon2id", "m_kib": 65536, "t": 3, "p": 1, "v": 1})
}

#[derive(Deserialize)]
pub struct StartReq {
    handle: String,
    blinded: Option<String>,
    #[serde(default)]
    recovery_only: bool,
}

#[derive(Serialize)]
pub struct StartResp {
    identity: String,
    mode: String,
    kdf: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    evaluated: Option<String>,
    generation: Option<String>,
}

fn locked_response() -> Response {
    err(
        StatusCode::LOCKED,
        "too many wrong attempts; this backup can no longer be opened",
    )
}

pub async fn restore_start(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<StartReq>,
) -> Response {
    let Some(handle) = protocol::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let blinded = req.blinded.as_deref().and_then(hex32);
    let (row, mut tx) = match vault_by_handle(&st, &handle).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // Key-only clients must not probe password backups through the decoy OPRF
    // path: neither vault access nor guess accounting is needed. Use the same
    // response for a password backup, an account without one, and an unknown @.
    if req.recovery_only && row.as_ref().is_none_or(|row| row.mode != "recovery_key") {
        return err(StatusCode::BAD_REQUEST, "recovery key backup unavailable");
    }
    let decoy_identity: Option<String> = if row.is_none() {
        match sqlx::query_scalar("SELECT id FROM identities WHERE handle = $1")
            .bind(&handle)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(identity) => identity,
            Err(_) => return unavailable(),
        }
    } else {
        None
    };
    let decoy = |st: &Shared| -> Response {
        // No backup here: answer exactly like a password vault would.
        let v = match vault(st) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let Some(b) = blinded else {
            return err(StatusCode::BAD_REQUEST, "bad point");
        };
        let Some(e) = v.evaluate_decoy(&handle, &b) else {
            return err(StatusCode::BAD_REQUEST, "bad point");
        };
        let (fake, generation) = v.decoy_metadata(&handle);
        Json(StartResp {
            identity: decoy_identity.clone().unwrap_or_else(|| hex::encode(fake)),
            mode: "passphrase".into(),
            kdf: default_kdf(),
            evaluated: Some(hex::encode(e)),
            generation: Some(hex::encode(generation)),
        })
        .into_response()
    };
    let Some(row) = row else {
        return decoy(&st);
    };
    if row.mode != "recovery_key" {
        if let Err(r) = vault(&st) {
            return r;
        }
    }
    if row.locked && row.mode != "recovery_key" {
        return locked_response();
    }
    match row.mode.as_str() {
        "recovery_key" => Json(StartResp {
            identity: row.identity,
            mode: row.mode,
            kdf: row.kdf,
            evaluated: None,
            generation: row.generation,
        })
        .into_response(),
        _ => {
            let (Ok(vault), Some(sealed), Some(b)) = (vault(&st), row.oprf_key.as_ref(), blinded)
            else {
                return err(StatusCode::BAD_REQUEST, "bad point");
            };
            let locked = match spend_guess(&mut tx, &row.identity, true).await {
                Ok(locked) => locked,
                Err(r) => return r,
            };
            if tx.commit().await.is_err() {
                return unavailable();
            }
            if locked {
                return locked_response();
            }
            let Some(e) = vault.evaluate(sealed, &row.identity, &b) else {
                return err(StatusCode::BAD_REQUEST, "bad point");
            };
            Json(StartResp {
                identity: row.identity,
                mode: row.mode,
                kdf: row.kdf,
                evaluated: Some(hex::encode(e)),
                generation: row.generation,
            })
            .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct OpenReq {
    handle: String,
    auth_key: String,
    generation: Option<String>,
}

#[derive(Serialize)]
pub struct OpenResp {
    wrapped_key: String,
    size: i64,
    generation: Option<String>,
}

#[derive(Deserialize)]
pub struct EnrollReq {
    handle: String,
    auth_key: String,
    generation: String,
    device: String,
    cert: String,
    sig: String,
}

/// A root certificate is necessary, but recovery authority must authorize its enrollment.
pub async fn restore_enroll(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<EnrollReq>,
) -> Response {
    let Some(handle) = protocol::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let (row, mut tx) = match authorize(&st, &handle, &req.auth_key, Some(&req.generation)).await {
        Ok(authorized) => authorized,
        Err(r) => return r,
    };
    if row.blob_bytes.is_none() {
        return err(StatusCode::NOT_FOUND, "no backup yet");
    }
    if !verify_sig(
        &req.device,
        &protocol::backup_enroll_message(&row.identity, &req.device, &req.cert, &req.generation),
        &req.sig,
    ) {
        return err(StatusCode::FORBIDDEN, "invalid device enrollment");
    }
    match crate::db::enroll_device(&mut tx, &row.identity, &req.device, &req.cert).await {
        Ok(()) => {}
        Err(crate::db::EnrollmentError::Database(_)) => return unavailable(),
        Err(_) => return err(StatusCode::FORBIDDEN, "device enrollment refused"),
    }
    if tx.commit().await.is_err() {
        return unavailable();
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Checks `auth_key` against the vault; `Ok` resets the guess counter.
async fn authorize(
    st: &Shared,
    handle: &str,
    auth_key: &str,
    generation: Option<&str>,
) -> Result<(VaultRow, Transaction<'static, Postgres>), Response> {
    let wrong = || err(StatusCode::FORBIDDEN, "wrong password or key");
    let (row, mut tx) = vault_by_handle(st, handle).await?;
    let Some(row) = row else {
        return Err(
            if st.backup_settings.dev_allow_unauthenticated_password_backup {
                wrong()
            } else {
                password_disabled()
            },
        );
    };
    if row.mode != "recovery_key" {
        vault(st)?;
    }
    if row.locked && row.mode != "recovery_key" {
        return Err(locked_response());
    }
    let ok = hex::decode(auth_key)
        .ok()
        .is_some_and(|k| ct_eq(&Sha256::digest(&k), &row.verifier));
    if !ok {
        // A random 64-digit key already resists guessing. Public failed attempts must
        // never destroy someone else's ability to recover with that key.
        if row.mode == "recovery_key" {
            return Err(wrong());
        }
        // The open after an evaluation is that evaluation's guess; any other failed open
        // costs one, so skipping the OPRF step never buys free guesses.
        let armed: bool = sqlx::query_scalar(
            "UPDATE backup_vaults v SET armed = false FROM backup_vaults old
             WHERE v.identity = $1 AND old.identity = v.identity RETURNING old.armed",
        )
        .bind(&row.identity)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
        let locked = if armed {
            false
        } else {
            spend_guess(&mut tx, &row.identity, false).await?
        };
        tx.commit().await.map_err(|_| unavailable())?;
        return Err(if locked { locked_response() } else { wrong() });
    }
    if row.generation.as_deref() != generation {
        return Err(err(
            StatusCode::CONFLICT,
            "backup configuration changed; restart restore",
        ));
    }
    sqlx::query(
        "UPDATE backup_vaults SET guesses = 0, armed = false, locked = false WHERE identity = $1",
    )
    .bind(&row.identity)
    .execute(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    Ok((row, tx))
}

pub async fn restore_open(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Json(req): Json<OpenReq>,
) -> Response {
    let Some(handle) = protocol::normalize_handle(&req.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    match authorize(&st, &handle, &req.auth_key, req.generation.as_deref()).await {
        Ok((row, tx)) => {
            if tx.commit().await.is_err() {
                return unavailable();
            }
            match row.blob_bytes {
                Some(size) => Json(OpenResp {
                    wrapped_key: hex::encode(row.wrapped_key),
                    size,
                    generation: row.generation,
                })
                .into_response(),
                None => err(StatusCode::NOT_FOUND, "no backup uploaded yet"),
            }
        }
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct BlobQuery {
    handle: String,
    generation: Option<String>,
}

pub async fn restore_blob(
    State(st): State<Shared>,
    crate::ClientIp(ip): crate::ClientIp,
    Query(q): Query<BlobQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(handle) = protocol::normalize_handle(&q.handle) else {
        return err(StatusCode::BAD_REQUEST, "bad handle");
    };
    if let Err(r) = restore_limit(&st, &ip, &handle) {
        return r;
    }
    let auth = headers
        .get("x-zoen-backup-auth")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (row, tx) = match authorize(&st, &handle, auth, q.generation.as_deref()).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let path = object_key(&row.identity, row.blob_key.as_deref());
    // Keep the snapshot row locked through headers and the entire bounded body fetch.
    match tokio::time::timeout(st.backup_settings.storage_timeout, async {
        st.blobs.get(&path).await?.bytes().await
    })
    .await
    {
        Ok(Ok(b)) => {
            if tx.commit().await.is_err() {
                return unavailable();
            }
            ([("content-type", "application/octet-stream")], b).into_response()
        }
        Ok(Err(object_store::Error::NotFound { .. })) => err(StatusCode::NOT_FOUND, "missing"),
        Ok(Err(_)) | Err(_) => err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable"),
    }
}
