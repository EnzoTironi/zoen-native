use super::*;
use protocol::provider_auth::{ProviderBindRequest, ProviderBindResponse};
use relay_admission::auth::enrollment::{Proof, ProofUse};
use sqlx::{PgPool, Row};

#[cfg(test)]
mod provider_fixture;

pub(super) async fn challenge(pool: &PgPool, id: &str) -> Result<Option<Challenge>, sqlx::Error> {
    let row=sqlx::query("SELECT provider,client,audience,presenter,origin,nonce,state,created_ms,expires_ms FROM provider_challenges WHERE id=$1 AND consumed_ms IS NULL")
        .bind(id).fetch_optional(pool).await?;
    row.map(|row| {
        let provider = match row.try_get::<String, _>("provider")?.as_str() {
            "apple" => Provider::Apple,
            "google" => Provider::Google,
            _ => return Err(sqlx::Error::Protocol("invalid provider challenge".into())),
        };
        let client = match row.try_get::<String, _>("client")?.as_str() {
            "ios" => Client::Ios,
            "android" => Client::Android,
            "web" => Client::Web,
            _ => return Err(sqlx::Error::Protocol("invalid provider client".into())),
        };
        Ok(Challenge {
            provider,
            client,
            audience: row.try_get("audience")?,
            presenter: row.try_get("presenter")?,
            origin: row.try_get("origin")?,
            nonce: row.try_get("nonce")?,
            state: row.try_get("state")?,
            created_ms: row.try_get("created_ms")?,
            expires_ms: row.try_get("expires_ms")?,
        })
    })
    .transpose()
}
pub(super) async fn issue(
    pool: &PgPool,
    id: &str,
    challenge: &Challenge,
    verified: &jwt::Verified,
    proof: &str,
    expires: i64,
    now: i64,
) -> Result<Account, Failure> {
    let mut tx = pool.begin().await.map_err(unavailable)?;
    let consumed=sqlx::query("UPDATE provider_challenges SET consumed_ms=$2 WHERE id=$1 AND consumed_ms IS NULL AND expires_ms>$2 AND state=$3")
        .bind(id).bind(now).bind(&challenge.state).execute(&mut *tx).await.map_err(unavailable)?;
    if consumed.rows_affected() != 1 {
        return Err(error(StatusCode::UNAUTHORIZED, "challenge_expired"));
    }
    sqlx::query("INSERT INTO provider_proofs(token_hash,challenge,provider,issuer,subject,origin,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(hash(proof)).bind(id).bind(challenge.provider.name()).bind(&verified.issuer).bind(&verified.subject).bind(&challenge.origin).bind(expires).execute(&mut *tx).await.map_err(unavailable)?;
    let bound=sqlx::query_as::<_,(String,String)>("SELECT b.identity,i.handle FROM provider_bindings b JOIN identities i ON i.id=b.identity WHERE b.provider=$1 AND b.issuer=$2 AND b.subject=$3")
        .bind(challenge.provider.name()).bind(&verified.issuer).bind(&verified.subject).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    tx.commit().await.map_err(unavailable)?;
    Ok(match bound {
        Some((identity_id, handle)) => Account::Bound {
            identity_id,
            handle,
        },
        None => Account::Unbound,
    })
}

pub(super) async fn bind(
    pool: &PgPool,
    request: &ProviderBindRequest,
    origin: Option<&str>,
    now: i64,
) -> Result<ProviderBindResponse, Failure> {
    let prepared = enrollment::prepare_bind(request)?;
    let profile = &request.profile;
    let handle = &prepared.lookup().handle;
    let mut tx = pool.begin().await.map_err(unavailable)?;
    sqlx::query(
        "SELECT set_config('lock_timeout','3s',true),set_config('statement_timeout','3s',true)",
    )
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    let row=sqlx::query("SELECT provider,issuer,subject,origin,expires_ms,consumed_ms,bound_fingerprint,bound_identity,bound_device,bound_handle FROM provider_proofs WHERE token_hash=$1 FOR UPDATE")
        .bind(&prepared.lookup().token_hash).fetch_optional(&mut *tx).await.map_err(unavailable)?.ok_or_else(||error(StatusCode::UNAUTHORIZED,"proof_expired"))?;
    let proof = Proof {
        token_hash: prepared.lookup().token_hash.clone(),
        provider: row.try_get("provider").map_err(unavailable)?,
        issuer: row.try_get("issuer").map_err(unavailable)?,
        subject: row.try_get("subject").map_err(unavailable)?,
        origin: row.try_get("origin").map_err(unavailable)?,
        expires_ms: row.try_get("expires_ms").map_err(unavailable)?,
        consumed: row
            .try_get::<Option<i64>, _>("consumed_ms")
            .map_err(unavailable)?
            .is_some(),
        bound_fingerprint: row.try_get("bound_fingerprint").map_err(unavailable)?,
        bound_identity: row.try_get("bound_identity").map_err(unavailable)?,
        bound_device: row.try_get("bound_device").map_err(unavailable)?,
        bound_handle: row.try_get("bound_handle").map_err(unavailable)?,
    };
    if enrollment::admit_proof(&prepared, Some(&proof), origin, now)? == ProofUse::Retry {
        let active=sqlx::query_scalar::<_,bool>("SELECT identity=$2 AND cert=$3 AND revoked_at IS NULL FROM devices WHERE device=$1 FOR SHARE").bind(&request.device).bind(&profile.id).bind(&request.certificate).fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let response = enrollment::finish_retry(&prepared, &proof, active == Some(true))?;
        tx.commit().await.map_err(unavailable)?;
        return Ok(response);
    }
    let provider = &proof.provider;
    let issuer = &proof.issuer;
    let subject = &proof.subject;
    // Serialize competing first binds of the same provider subject or identity.
    // The generic registration path still owns all identity/device invariants.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("provider:{provider}:{issuer}:{subject}"))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("identity:{}", profile.id))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    let binding=sqlx::query_scalar::<_,String>("SELECT identity FROM provider_bindings WHERE provider=$1 AND issuer=$2 AND subject=$3 FOR UPDATE").bind(provider).bind(issuer).bind(subject).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    enrollment::check_provider_binding(binding.as_deref(), &profile.id)?;
    let existing_provider = sqlx::query_scalar::<_, String>(
        "SELECT subject FROM provider_bindings WHERE identity=$1 AND provider=$2 FOR UPDATE",
    )
    .bind(&profile.id)
    .bind(provider)
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?;
    enrollment::check_identity_provider(existing_provider.as_deref(), subject)?;
    let registered = sqlx::query_as::<_, (String, String)>(
        "SELECT handle,kind FROM identities WHERE id=$1 FOR UPDATE",
    )
    .bind(&profile.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?;
    let final_handle = if let Some((stored_handle, kind)) = registered {
        enrollment::check_registered_kind(&kind)?;
        let active=sqlx::query_scalar::<_,bool>("SELECT identity=$2 AND cert=$3 AND revoked_at IS NULL FROM devices WHERE device=$1 FOR SHARE").bind(&request.device).bind(&profile.id).bind(&request.certificate).fetch_optional(&mut *tx).await.map_err(unavailable)?;
        enrollment::existing_identity_handle(&stored_handle, active == Some(true))?
    } else {
        crate::db::register(
            &mut tx,
            profile,
            handle,
            &request.device,
            &request.certificate,
            true,
        )
        .await
        .map_err(|reason| enrollment::registration_failure(&reason))?;
        handle.clone()
    };
    sqlx::query("INSERT INTO provider_bindings(provider,issuer,subject,identity) VALUES($1,$2,$3,$4) ON CONFLICT(provider,issuer,subject) DO NOTHING")
        .bind(provider).bind(issuer).bind(subject).bind(&profile.id).execute(&mut *tx).await.map_err(unavailable)?;
    sqlx::query("UPDATE provider_proofs SET consumed_ms=$2,bound_fingerprint=$3,bound_identity=$4,bound_device=$5,bound_handle=$6 WHERE token_hash=$1")
        .bind(&prepared.lookup().token_hash).bind(now).bind(prepared.fingerprint()).bind(&profile.id).bind(&request.device).bind(&final_handle).execute(&mut *tx).await.map_err(unavailable)?;
    tx.commit().await.map_err(unavailable)?;
    Ok(ProviderBindResponse {
        identity_id: profile.id.clone(),
        handle: final_handle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use event_log::Signer;
    use protocol::provider_auth::provider_bind_message;
    use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
    use types::{Identity, IdentityKind};

    fn request(seed: u8, proof: &str, handle: &str) -> ProviderBindRequest {
        request_with_device(seed, seed + 1, proof, handle)
    }
    fn request_with_device(
        seed: u8,
        device_seed: u8,
        proof: &str,
        handle: &str,
    ) -> ProviderBindRequest {
        let identity = Signer::from_secret(&[seed; 32]);
        let device = Signer::from_secret(&[device_seed; 32]);
        let profile = Identity {
            id: identity.id(),
            kind: IdentityKind::Person,
            name: String::new(),
            handle: handle.into(),
            tint_hex: "#888888".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
            owner_proof: None,
        };
        let certificate = identity.sign(&event_log::device_cert_message(&device.id()));
        let signature = device
            .sign(&provider_bind_message(proof, &profile, &device.id(), &certificate).unwrap());
        ProviderBindRequest {
            proof_token: proof.into(),
            profile,
            device: device.id(),
            certificate,
            signature,
        }
    }
    async fn proof(pool: &PgPool, id: &str, subject: &str, proof: &str) {
        sqlx::query("INSERT INTO provider_challenges(id,provider,client,audience,nonce,state,created_ms,expires_ms) VALUES($1,'google','ios','aud','nonce','state',0,999999)").bind(id).execute(pool).await.unwrap();
        let challenge = super::challenge(pool, id).await.unwrap().unwrap();
        issue(
            pool,
            id,
            &challenge,
            &jwt::Verified {
                issuer: "https://accounts.google.com".into(),
                subject: subject.into(),
                name: None,
            },
            proof,
            999999,
            1,
        )
        .await
        .unwrap();
    }

    async fn exercise_binding_contract(pool: PgPool) {
        // Two independently verified sessions race to bind one provider subject.
        // The loser must not leave an orphan identity or device behind.
        proof(&pool, "challenge1", "same-subject", &"11".repeat(32)).await;
        proof(&pool, "challenge2", "same-subject", &"22".repeat(32)).await;
        let first = request(60, &"11".repeat(32), "first_person");
        let second = request(70, &"22".repeat(32), "second_person");
        let (a, b) = tokio::join!(bind(&pool, &first, None, 2), bind(&pool, &second, None, 2));
        assert_ne!(a.is_ok(), b.is_ok());
        let winner = if a.is_ok() { &first } else { &second };
        let winner_seed = if a.is_ok() { 60 } else { 70 };
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM identities")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM devices")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM provider_bindings")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert!(bind(&pool, winner, None, 3).await.is_ok());
        // A consumed proof remains an exact receipt after expiry and a profile
        // rename. It returns its original handle rather than editing the profile.
        sqlx::query("UPDATE identities SET handle='renamed_person' WHERE id=$1")
            .bind(&winner.profile.id)
            .execute(&pool)
            .await
            .unwrap();
        let retry = bind(&pool, winner, None, 1_000_000).await.unwrap();
        assert_eq!(retry.handle, winner.profile.handle);

        // An existing identity still needs its exact active device; the proof
        // cannot enroll another root-certified device or replace the profile.
        let fresh_proof = "55".repeat(32);
        proof(&pool, "existing", "same-subject", &fresh_proof).await;
        let other_device = request_with_device(winner_seed, 91, &fresh_proof, "ignored_profile");
        assert_eq!(
            bind(&pool, &other_device, None, 4)
                .await
                .err()
                .unwrap()
                .code,
            "link_device_first"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM devices")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        let profile_before: serde_json::Value =
            sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
                .bind(&winner.profile.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let existing = request(winner_seed, &fresh_proof, "ignored_profile");
        assert_eq!(
            bind(&pool, &existing, None, 5).await.unwrap().handle,
            "renamed_person"
        );
        let profile_after: serde_json::Value =
            sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
                .bind(&winner.profile.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(profile_before, profile_after);

        // Unique handle failure rolls back both first enrollment and proof use.
        let collision_proof = "66".repeat(32);
        proof(&pool, "handle_collision", "new-subject", &collision_proof).await;
        let collision = request(80, &collision_proof, "renamed_person");
        assert_eq!(
            bind(&pool, &collision, None, 6).await.err().unwrap().code,
            "handle_taken"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM identities")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM devices")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert!(sqlx::query_scalar::<_, bool>(
            "SELECT consumed_ms IS NULL FROM provider_proofs WHERE token_hash=$1"
        )
        .bind(hash(&collision_proof))
        .fetch_one(&pool)
        .await
        .unwrap());

        // A valid signature with different profile bytes cannot reuse a receipt,
        // even after expiry or subsequent revocation.
        let changed = request(winner_seed, &winner.proof_token, "different_profile");
        assert_eq!(
            bind(&pool, &changed, None, 1_000_000)
                .await
                .err()
                .unwrap()
                .code,
            "proof_already_used"
        );
        sqlx::query("UPDATE devices SET revoked_at=now() WHERE device=$1")
            .bind(&winner.device)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            bind(&pool, winner, None, 4).await.err().unwrap().status,
            StatusCode::FORBIDDEN
        );
        assert!(sqlx::query_scalar::<_, bool>(
            "SELECT revoked_at IS NOT NULL FROM devices WHERE device=$1"
        )
        .bind(&winner.device)
        .fetch_one(&pool)
        .await
        .unwrap());
        assert_eq!(
            bind(&pool, winner, None, 1_000_000)
                .await
                .err()
                .unwrap()
                .code,
            "device_revoked"
        );

        // A challenge cannot issue two proofs, even under concurrent verification.
        sqlx::query("INSERT INTO provider_challenges(id,provider,client,audience,nonce,state,created_ms,expires_ms) VALUES('race','google','ios','aud','nonce','state',0,999999)").execute(&pool).await.unwrap();
        let challenge = super::challenge(&pool, "race").await.unwrap().unwrap();
        let verified = jwt::Verified {
            issuer: "https://accounts.google.com".into(),
            subject: "other".into(),
            name: None,
        };
        let proof_a = "33".repeat(32);
        let proof_b = "44".repeat(32);
        let (a, b) = tokio::join!(
            issue(&pool, "race", &challenge, &verified, &proof_a, 999999, 2),
            issue(&pool, "race", &challenge, &verified, &proof_b, 999999, 2)
        );
        assert_ne!(a.is_ok(), b.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires an isolated schema in ZOEN_TEST_PG"]
    async fn database_proofs_are_single_use_binds_are_atomic_and_revoked_devices_stay_revoked(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let url = std::env::var("ZOEN_TEST_PG").expect("ZOEN_TEST_PG");
        let mut admin = PgConnection::connect(&url).await.unwrap();
        let schema = format!(
            "auth_test_{}",
            &hex::encode(Signer::generate()?.secret())[..16]
        );
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let path = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _| {
                let sql = format!("SET search_path TO {path}");
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(sql))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        // Catch assertion panics through the task so cleanup still drops only
        // this disposable schema before propagating the fixture failure.
        let outcome = tokio::spawn(exercise_binding_contract(pool.clone())).await;

        pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&mut admin)
            .await
            .unwrap();
        outcome.expect("isolated provider enrollment fixture failed after schema cleanup");
        Ok(())
    }
}
