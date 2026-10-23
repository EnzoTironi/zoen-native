use super::*;

// ───────────────────────────── device side ─────────────────────────────

#[derive(Deserialize)]
pub struct OprfReq {
    blinded: String,
    generation: String,
}

#[derive(Serialize)]
pub struct OprfResp {
    evaluated: String,
}

/// Starts a password setup: a new pending OPRF key, evaluated on the device's blinded input.
pub async fn oprf(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let vault = match vault(&st) {
        Ok(v) => v.clone(),
        Err(r) => return r,
    };
    let (identity, mut tx) = match signed_identity(&st, &headers, "oprf", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let Ok(req) = serde_json::from_slice::<OprfReq>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    let Some(blinded) = hex32(&req.blinded) else {
        return err(StatusCode::BAD_REQUEST, "bad point");
    };
    if hex32(&req.generation).is_none() {
        return err(StatusCode::BAD_REQUEST, "bad generation");
    }
    let Ok(sealed) = vault.new_key(&identity) else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "vault unavailable");
    };
    let Some(evaluated) = vault.evaluate(&sealed, &identity, &blinded) else {
        return err(StatusCode::BAD_REQUEST, "bad point");
    };
    let r = sqlx::query(
        "INSERT INTO backup_pending (identity, pending_key, generation, device) VALUES ($1, $2, $3, $4)
         ON CONFLICT (identity) DO UPDATE SET pending_key = $2, generation = $3,
             device = $4, created_at = now()",
    )
    .bind(&identity)
    .bind(&sealed)
    .bind(&req.generation)
    .bind(headers.get("x-zoen-device").and_then(|v| v.to_str().ok()).unwrap_or(""))
    .execute(&mut *tx)
    .await;
    if r.is_err() || tx.commit().await.is_err() {
        return unavailable();
    }
    Json(OprfResp {
        evaluated: hex::encode(evaluated),
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct VaultReq {
    generation: String,
    mode: String,
    verifier: String,
    wrapped_key: String,
    kdf: serde_json::Value,
}

/// Stages a configuration. The previous backup remains restorable until its upload commits.
pub async fn put_vault(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "vault", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let Ok(req) = serde_json::from_slice::<VaultReq>(&body) else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    let (Some(verifier), Ok(wrapped)) = (hex32(&req.verifier), hex::decode(&req.wrapped_key))
    else {
        return err(StatusCode::BAD_REQUEST, "bad request");
    };
    if wrapped.len() > 256 || hex32(&req.generation).is_none() {
        return err(StatusCode::BAD_REQUEST, "bad request");
    }
    let oprf_key: Option<Vec<u8>> = match req.mode.as_str() {
        "recovery_key" => None,
        "passphrase" => {
            if let Err(r) = vault(&st) {
                return r;
            }
            let pending: Option<Vec<u8>> = match sqlx::query_scalar(
                "DELETE FROM backup_pending WHERE identity = $1 AND generation = $2
                    AND device = $3 RETURNING pending_key",
            )
            .bind(&identity)
            .bind(&req.generation)
            .bind(
                headers
                    .get("x-zoen-device")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            )
            .fetch_optional(&mut *tx)
            .await
            {
                Ok(p) => p,
                Err(_) => return unavailable(),
            };
            match pending {
                Some(p) => Some(p),
                None => return err(StatusCode::CONFLICT, "start a password setup first"),
            }
        }
        _ => return err(StatusCode::BAD_REQUEST, "bad mode"),
    };
    let r = sqlx::query(
        "INSERT INTO backup_setups (identity, mode, oprf_key, verifier, wrapped_key, kdf, generation)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (identity) DO UPDATE SET mode = $2, oprf_key = $3, verifier = $4,
             wrapped_key = $5, kdf = $6, generation = $7, created_at = now()",
    )
    .bind(&identity)
    .bind(&req.mode)
    .bind(&oprf_key)
    .bind(verifier.as_slice())
    .bind(&wrapped)
    .bind(&req.kdf)
    .bind(&req.generation)
    .execute(&mut *tx)
    .await;
    if r.is_err() || tx.commit().await.is_err() {
        return unavailable();
    }
    tracing::info!(identity = %crate::pseudonym::pseudo(&identity), mode = %req.mode, "backup configuration staged");
    StatusCode::NO_CONTENT.into_response()
}

/// Stores the newest sealed backup (replaces the previous one).
pub async fn put_blob(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "blob", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    const PREFIX: usize = 8 + 32;
    if body.len() <= PREFIX || body.len() > MAX_BACKUP_BYTES + PREFIX {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "empty or too large");
    }
    if body.get(..8) != Some(BACKUP_UPLOAD_MAGIC.as_slice()) {
        return err(StatusCode::BAD_REQUEST, "backup generation required");
    }
    let generation = hex::encode(&body[8..PREFIX]);
    let body = body.slice(PREFIX..);
    let active = match sqlx::query(
        "SELECT generation, mode, locked, blob_key FROM backup_vaults WHERE identity = $1 FOR UPDATE",
    )
    .bind(&identity)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let setup = match sqlx::query(
        "SELECT mode, oprf_key, verifier, wrapped_key, kdf FROM backup_setups
         WHERE identity = $1 AND generation = $2",
    )
    .bind(&identity)
    .bind(&generation)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    let active_matches = active.as_ref().is_some_and(|row| {
        row.get::<Option<String>, _>("generation").as_deref() == Some(&generation)
            && !row.get::<bool, _>("locked")
    });
    if setup.is_none() && !active_matches {
        return err(
            StatusCode::CONFLICT,
            "backup configuration changed; set it up again on this device",
        );
    }
    let mode = setup
        .as_ref()
        .or(active.as_ref())
        .expect("matching setup or active vault")
        .get::<String, _>("mode");
    if mode == "passphrase" {
        if let Err(r) = vault(&st) {
            return r;
        }
    }
    let sha = hex::encode(Sha256::digest(&body));
    let n = body.len();
    let mut object_id = [0u8; 32];
    if getrandom::getrandom(&mut object_id).is_err() {
        return unavailable();
    }
    let key = format!("backups/v2/{identity}/{}", hex::encode(object_id));
    let path = object_key(&identity, Some(&key));
    match tokio::time::timeout(
        st.backup_settings.storage_timeout,
        st.blobs.put(&path, PutPayload::from_bytes(body)),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "backup put failed");
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
        Err(_) => {
            // The remote PUT may still finish. This unique key is an orphan, never an
            // active pointer: publication only follows an acknowledged durable write.
            tracing::warn!("backup put timed out");
            return err(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable");
        }
    }
    let r = if let Some(setup) = setup {
        sqlx::query(
            "INSERT INTO backup_vaults (identity, mode, oprf_key, verifier, wrapped_key,
                kdf, generation, blob_sha, blob_bytes, blob_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (identity) DO UPDATE SET mode = $2, oprf_key = $3, verifier = $4,
                wrapped_key = $5, kdf = $6, generation = $7, blob_sha = $8, blob_bytes = $9,
                blob_key = $10, guesses = 0, armed = false, locked = false, updated_at = now()",
        )
        .bind(&identity)
        .bind(setup.get::<String, _>("mode"))
        .bind(setup.get::<Option<Vec<u8>>, _>("oprf_key"))
        .bind(setup.get::<Vec<u8>, _>("verifier"))
        .bind(setup.get::<Vec<u8>, _>("wrapped_key"))
        .bind(setup.get::<serde_json::Value, _>("kdf"))
        .bind(&generation)
        .bind(&sha)
        .bind(n as i64)
        .bind(&key)
        .execute(&mut *tx)
        .await
    } else {
        sqlx::query(
            "UPDATE backup_vaults SET blob_sha = $2, blob_bytes = $3, blob_key = $4, updated_at = now()
             WHERE identity = $1",
        )
        .bind(&identity)
        .bind(&sha)
        .bind(n as i64)
        .bind(&key)
        .execute(&mut *tx)
        .await
    };
    if r.is_err() {
        return unavailable();
    }
    if sqlx::query("DELETE FROM backup_setups WHERE identity = $1 AND generation = $2")
        .bind(&identity)
        .bind(&generation)
        .execute(&mut *tx)
        .await
        .is_err()
        || tx.commit().await.is_err()
    {
        return unavailable();
    }
    if let Some(old) = active {
        let old_key: Option<String> = old.get("blob_key");
        let old_path = object_key(&identity, old_key.as_deref());
        if old_path != path {
            cleanup(&st, &old_path).await;
        }
    }
    tracing::info!(identity = %crate::pseudonym::pseudo(&identity), bytes = n, "backup stored");
    StatusCode::CREATED.into_response()
}

/// Turns the backup off: forgets the vault and the object.
pub async fn delete(State(st): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (identity, mut tx) = match signed_identity(&st, &headers, "delete", &body).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let old = match sqlx::query("DELETE FROM backup_vaults WHERE identity = $1 RETURNING blob_key")
        .bind(&identity)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(row) => row,
        Err(_) => return unavailable(),
    };
    for table in ["backup_pending", "backup_setups"] {
        let sql = format!("DELETE FROM {table} WHERE identity = $1");
        if sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&identity)
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return unavailable();
        }
    }
    if tx.commit().await.is_err() {
        return unavailable();
    }
    if let Some(row) = old {
        let key: Option<String> = row.get("blob_key");
        cleanup(&st, &object_key(&identity, key.as_deref())).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn cleanup(st: &Shared, path: &ObjPath) {
    match tokio::time::timeout(st.backup_settings.storage_timeout, st.blobs.delete(path)).await {
        Ok(Ok(())) | Ok(Err(object_store::Error::NotFound { .. })) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "backup cleanup failed; object retained for GC"),
        Err(_) => tracing::warn!("backup cleanup timed out; object retained for GC"),
    }
}
