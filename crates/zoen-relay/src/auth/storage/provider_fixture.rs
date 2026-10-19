//! Disposable native SQL oracle. Its signed assertions/keys are synthetic; this
//! module is absent from production and never exposes a Verified injection API.

use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use event_log::Signer;
use protocol::provider_auth::provider_bind_message;
use relay_admission::auth::{verify_with_key, Jwk, VerificationContext};
use ring::{
    rand::SystemRandom,
    signature::{RsaKeyPair, RSA_PKCS1_SHA256},
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, Connection, PgConnection};
use std::io::Write;
use types::{Identity, IdentityKind};

const ORIGIN: &str = "https://app.example";

fn key() -> Jwk {
    serde_json::from_str(include_str!(
        "../../../../relay-admission/src/auth/fixtures/rsa-test-only.jwk.json"
    ))
    .unwrap()
}

fn assertion(challenge: &Challenge, subject: &str, expires_secs: i64) -> (String, Jwk) {
    let key = key();
    let signer = RsaKeyPair::from_pkcs8(include_bytes!(
        "../../../../relay-admission/src/auth/fixtures/rsa-test-only.pk8"
    ))
    .unwrap();
    let input = format!("{}.{}", B64.encode(serde_json::to_vec(&json!({"alg":"RS256","kid":key.kid,"typ":"JWT"})).unwrap()), B64.encode(serde_json::to_vec(&json!({
        "iss":"https://accounts.google.com","sub":subject,"aud":challenge.audience,
        "azp":challenge.presenter,"nonce":challenge.nonce,"iat":1000,"exp":expires_secs,"name":"Display hint"
    })).unwrap()));
    let mut signature = vec![0; signer.public().modulus_len()];
    signer
        .sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            input.as_bytes(),
            &mut signature,
        )
        .unwrap();
    (format!("{input}.{}", B64.encode(signature)), key)
}

fn binding(root_seed: u8, device_seed: u8, proof: &str, handle: &str) -> ProviderBindRequest {
    let root = Signer::from_secret(&[root_seed; 32]);
    let device = Signer::from_secret(&[device_seed; 32]);
    let profile = Identity {
        id: root.id(),
        kind: IdentityKind::Person,
        name: String::new(),
        handle: handle.into(),
        tint_hex: "#888888".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    };
    let certificate = root.sign(&event_log::device_cert_message(&device.id()));
    let signature =
        device.sign(&provider_bind_message(proof, &profile, &device.id(), &certificate).unwrap());
    ProviderBindRequest {
        proof_token: proof.into(),
        profile,
        device: device.id(),
        certificate,
        signature,
    }
}

async fn rows(pool: &PgPool, query: &'static str) -> Vec<Value> {
    sqlx::query_scalar(query).fetch_all(pool).await.unwrap()
}

async fn snapshot(pool: &PgPool) -> Value {
    json!({
        "identities":rows(pool,"SELECT profile FROM identities ORDER BY id").await,
        "devices":rows(pool,"SELECT jsonb_build_object('device',device,'identity',identity,'cert',cert,'revoked',revoked_at IS NOT NULL) FROM devices ORDER BY device").await,
        "challenges":rows(pool,"SELECT jsonb_build_object('id',id,'provider',provider,'client',client,'audience',audience,'presenter',presenter,'origin',origin,'nonce',nonce,'state',state,'created_ms',created_ms::text,'expires_ms',expires_ms::text,'consumed_ms',consumed_ms::text) FROM provider_challenges ORDER BY id").await,
        "proofs":rows(pool,"SELECT jsonb_build_object('token_hash',token_hash,'challenge',challenge,'provider',provider,'issuer',issuer,'subject',subject,'origin',origin,'expires_ms',expires_ms::text,'consumed_ms',consumed_ms::text,'bound_fingerprint',bound_fingerprint,'bound_identity',bound_identity,'bound_device',bound_device,'bound_handle',bound_handle) FROM provider_proofs ORDER BY token_hash").await,
        "bindings":rows(pool,"SELECT jsonb_build_object('provider',provider,'issuer',issuer,'subject',subject,'identity',identity) FROM provider_bindings ORDER BY provider,issuer,subject").await,
    })
}

async fn seed_challenge(pool: &PgPool, id: &str) -> Challenge {
    sqlx::query("INSERT INTO provider_challenges(id,provider,client,audience,presenter,origin,nonce,state,created_ms,expires_ms) VALUES($1,'google','web','audience','ios-client',$2,$3,$4,1000000,1300000)")
        .bind(id).bind(ORIGIN).bind("aa".repeat(32)).bind("bb".repeat(32)).execute(pool).await.unwrap();
    challenge(pool, id).await.unwrap().unwrap()
}

async fn seed_proof(pool: &PgPool, id: &str, proof_token: &str, subject: &str) {
    let challenge = seed_challenge(pool, id).await;
    let (token, key) = assertion(&challenge, subject, 1600);
    let verified = verified(&token, &key, &challenge, 1_000_000);
    issue(
        pool,
        id,
        &challenge,
        &verified,
        proof_token,
        1_300_000,
        1_000_000,
    )
    .await
    .unwrap();
}

fn verified(token: &str, key: &Jwk, challenge: &Challenge, now: i64) -> jwt::Verified {
    verify_with_key(
        token,
        key,
        &VerificationContext {
            provider: challenge.provider,
            audience: &challenge.audience,
            presenter: challenge.presenter.as_deref(),
            nonce: &challenge.nonce,
            created_ms: challenge.created_ms,
        },
        now,
    )
    .unwrap()
}

fn expected<T: serde::Serialize>(result: Result<T, Failure>) -> Value {
    match result {
        Ok(value) => json!({"status":200,"body":value}),
        Err(error) => json!({"status":error.status.as_u16(),"body":{"error":error.code}}),
    }
}

async fn bind_case(
    pool: &PgPool,
    label: &str,
    request: &ProviderBindRequest,
    origin: Option<&str>,
    now: i64,
) -> Value {
    let initial = snapshot(pool).await;
    let outcome = expected(bind(pool, request, origin, now).await);
    json!({"label":label,"initial_state":initial,"steps":[{
        "label":label,"op":"bind","body":request,"origin":origin,"now_ms":now.to_string(),"entropy":[],
        "expected":outcome,"state_after":snapshot(pool).await
    }]})
}

async fn issue_case(
    pool: &PgPool,
    label: &str,
    id: &str,
    challenge: &Challenge,
    proof_token: &str,
    clocks: (i64, i64, i64),
) -> Value {
    issue_case_subject(
        pool,
        label,
        id,
        challenge,
        proof_token,
        clocks,
        "provider-subject",
    )
    .await
}

async fn issue_case_subject(
    pool: &PgPool,
    label: &str,
    id: &str,
    challenge: &Challenge,
    proof_token: &str,
    clocks: (i64, i64, i64),
    subject: &str,
) -> Value {
    let (verification_now, issue_now, expires_secs) = clocks;
    let initial = snapshot(pool).await;
    let (token, key) = assertion(challenge, subject, expires_secs);
    let verified = verified(&token, &key, challenge, verification_now);
    let expires = issue_now + enrollment::PROOF_LIFETIME_MS;
    let result = issue(pool, id, challenge, &verified, proof_token, expires, issue_now).await.map(|account| {
        json!({"proof_token":proof_token,"expires_ms":expires,"account":account,"name":verified.name})
    });
    json!({"label":label,"initial_state":initial,"steps":[{
        "label":label,"op":"issue","body":{"challenge_id":id,"id_token":token,"state":challenge.state},
        "origin":challenge.origin,"now_ms":verification_now.to_string(),"verification_now_ms":verification_now.to_string(),"issue_now_ms":issue_now.to_string(),
        "entropy":[proof_token],"expected":expected(result),"state_after":snapshot(pool).await
    }]})
}

async fn produce(pool: PgPool) -> Value {
    let mut scenarios = Vec::new();
    let id = "01".repeat(32);
    let challenge = seed_challenge(&pool, &id).await;
    let first_token = "11".repeat(32);
    scenarios.push(
        issue_case(
            &pool,
            "issue-first",
            &id,
            &challenge,
            &first_token,
            (1_000_000, 1_000_001, 1600),
        )
        .await,
    );
    scenarios.push(
        issue_case(
            &pool,
            "issue-consumed",
            &id,
            &challenge,
            &"12".repeat(32),
            (1_000_002, 1_000_003, 1600),
        )
        .await,
    );
    let first = binding(60, 61, &first_token, "first_person");
    scenarios.push(bind_case(&pool, "bind-first", &first, Some(ORIGIN), 1_000_004).await);
    scenarios.push(bind_case(&pool, "bind-retry", &first, Some(ORIGIN), 1_000_005).await);

    sqlx::query("UPDATE identities SET handle=$2,profile=jsonb_set(profile,'{handle}',to_jsonb($2::text)) WHERE id=$1")
        .bind(&first.profile.id).bind("renamed_person").execute(&pool).await.unwrap();
    scenarios.push(
        bind_case(
            &pool,
            "bind-expired-original-handle",
            &first,
            Some(ORIGIN),
            1_700_000,
        )
        .await,
    );
    let changed = binding(60, 61, &first_token, "different_profile");
    scenarios.push(
        bind_case(
            &pool,
            "bind-changed-payload",
            &changed,
            Some(ORIGIN),
            1_700_001,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-origin-mismatch",
            &first,
            Some("https://other.example"),
            1_700_001,
        )
        .await,
    );

    for (code, subject) in [
        ("22", "provider-subject"),
        ("33", "provider-subject"),
        ("44", "provider-subject"),
        ("55", "other-subject"),
        ("66", "third-subject"),
        ("77", "fourth-subject"),
    ] {
        seed_proof(
            &pool,
            &format!("{code}00").repeat(16),
            &code.repeat(32),
            subject,
        )
        .await;
    }
    scenarios.push(
        bind_case(
            &pool,
            "bind-existing-active",
            &binding(60, 61, &"22".repeat(32), "ignored_profile"),
            Some(ORIGIN),
            1_000_010,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-existing-new-device",
            &binding(60, 91, &"33".repeat(32), "ignored_profile"),
            Some(ORIGIN),
            1_000_011,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-provider-collision",
            &binding(70, 71, &"44".repeat(32), "second_person"),
            Some(ORIGIN),
            1_000_012,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-handle-collision",
            &binding(70, 71, &"55".repeat(32), "renamed_person"),
            Some(ORIGIN),
            1_000_013,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-global-device-collision",
            &binding(80, 61, &"66".repeat(32), "third_person"),
            Some(ORIGIN),
            1_000_014,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-identity-provider-collision",
            &binding(60, 61, &"77".repeat(32), "ignored_profile"),
            Some(ORIGIN),
            1_000_015,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-expired-unused",
            &binding(70, 71, &"44".repeat(32), "second_person"),
            Some(ORIGIN),
            1_700_002,
        )
        .await,
    );
    scenarios.push(
        bind_case(
            &pool,
            "bind-missing-proof",
            &binding(70, 71, &"a0".repeat(32), "second_person"),
            Some(ORIGIN),
            1_000_016,
        )
        .await,
    );

    sqlx::query("UPDATE devices SET revoked_at=now() WHERE device=$1")
        .bind(&first.device)
        .execute(&pool)
        .await
        .unwrap();
    scenarios.push(bind_case(&pool, "bind-revoked-retry", &first, Some(ORIGIN), 1_700_003).await);
    seed_proof(
        &pool,
        &"8800".repeat(16),
        &"88".repeat(32),
        "provider-subject",
    )
    .await;
    scenarios.push(
        bind_case(
            &pool,
            "bind-revoked-fresh",
            &binding(60, 61, &"88".repeat(32), "ignored_profile"),
            Some(ORIGIN),
            1_000_017,
        )
        .await,
    );
    seed_proof(&pool, &"9900".repeat(16), &"99".repeat(32), "fifth-subject").await;
    scenarios.push(
        bind_case(
            &pool,
            "bind-revoked-global-device",
            &binding(90, 61, &"99".repeat(32), "fourth_person"),
            Some(ORIGIN),
            1_000_018,
        )
        .await,
    );

    let late_id = "a100".repeat(16);
    let late = seed_challenge(&pool, &late_id).await;
    scenarios.push(
        issue_case(
            &pool,
            "issue-token-expires-during-fetch",
            &late_id,
            &late,
            &"a1".repeat(32),
            (1_001_000, 1_003_000, 1002),
        )
        .await,
    );
    let expiry_id = "a200".repeat(16);
    let expiry = seed_challenge(&pool, &expiry_id).await;
    scenarios.push(
        issue_case(
            &pool,
            "issue-challenge-expires-during-fetch",
            &expiry_id,
            &expiry,
            &"a2".repeat(32),
            (1_299_000, 1_300_000, 1600),
        )
        .await,
    );

    // Ensure the oracle exercised the intended branches before publishing its
    // authoritative SQL outcomes, rather than accepting a misconfigured fixture.
    let outcomes = [
        (200, None),
        (401, Some("challenge_expired")),
        (200, None),
        (200, None),
        (200, None),
        (409, Some("proof_already_used")),
        (403, Some("origin_refused")),
        (200, None),
        (403, Some("link_device_first")),
        (409, Some("provider_already_bound")),
        (409, Some("handle_taken")),
        (409, Some("registration_refused")),
        (409, Some("identity_provider_already_bound")),
        (401, Some("proof_expired")),
        (401, Some("proof_expired")),
        (403, Some("device_revoked")),
        (403, Some("link_device_first")),
        (409, Some("registration_refused")),
        (200, None),
        (401, Some("challenge_expired")),
    ];
    assert_eq!(scenarios.len(), outcomes.len());
    for (scenario, (status, code)) in scenarios.iter().zip(outcomes) {
        let step = &scenario["steps"][0];
        assert_eq!(step["expected"]["status"], status, "{}", scenario["label"]);
        if let Some(code) = code {
            assert_eq!(step["expected"]["body"]["error"], code);
            assert_eq!(scenario["initial_state"], step["state_after"]);
        }
        let proof_token = step["body"]["proof_token"]
            .as_str()
            .or_else(|| step["entropy"][0].as_str());
        if let Some(token) = proof_token {
            assert!(
                !step["state_after"].to_string().contains(token),
                "opaque proof is not stored in plaintext"
            );
        }
    }
    assert_eq!(
        scenarios[4]["steps"][0]["expected"]["body"]["handle"],
        "first_person"
    );
    assert_eq!(
        scenarios[7]["steps"][0]["expected"]["body"]["handle"],
        "renamed_person"
    );
    assert_eq!(
        scenarios[7]["initial_state"]["identities"],
        scenarios[7]["steps"][0]["state_after"]["identities"]
    );

    json!({"schema":1,"scope":"Actual native issue/bind SQL outcomes and durable state only; synthetic assertions; HTTP extraction, JWKS transport/cache and challenge creation tested separately.",
        "settings":{"audiences":[{"provider":"google","client":"web","audience":{"id":"audience","presenter":"ios-client"}}],"origins":[ORIGIN]},
        "jwks":{"keys":[key()]},"scenarios":scenarios})
}

async fn android_scenarios(pool: &PgPool) -> Value {
    let id = "a3".repeat(32);
    sqlx::query("INSERT INTO provider_challenges(id,provider,client,audience,presenter,origin,nonce,state,created_ms,expires_ms) VALUES($1,'google','android','audience','android-client',NULL,$2,$3,1000000,1300000)")
        .bind(&id).bind("aa".repeat(32)).bind("bb".repeat(32)).execute(pool).await.unwrap();
    let android = challenge(pool, &id).await.unwrap().unwrap();
    assert_eq!(android.client, Client::Android);
    assert_eq!(android.origin, None);
    let proof = "a4".repeat(32);
    let issued = issue_case_subject(
        pool,
        "android-issue-first",
        &id,
        &android,
        &proof,
        (1_000_000, 1_000_001, 1600),
        "android-provider-subject",
    )
    .await;
    assert_eq!(issued["steps"][0]["expected"]["status"], 200);
    let consumed = issue_case_subject(
        pool,
        "android-issue-consumed",
        &id,
        &android,
        &"a5".repeat(32),
        (1_000_002, 1_000_003, 1600),
        "android-provider-subject",
    )
    .await;
    assert_eq!(
        consumed["steps"][0]["expected"],
        json!({"status":401,"body":{"error":"challenge_expired"}})
    );
    let bound = bind_case(
        pool,
        "android-bind-first",
        &binding(100, 101, &proof, "android_person"),
        None,
        1_000_004,
    )
    .await;
    assert_eq!(bound["steps"][0]["expected"]["status"], 200);
    json!({
        "settings":{"audiences":[{"provider":"google","client":"android","audience":{"id":"audience","presenter":"android-client"}}],"origins":[ORIGIN]},
        "scenarios":[issued,consumed,bound]
    })
}

#[tokio::test]
#[ignore = "requires isolated ZOEN_TEST_PG and a fresh ZOEN_PROVIDER_FIXTURE output path"]
async fn write_native_provider_enrollment_fixture() {
    let output = std::env::var("ZOEN_PROVIDER_FIXTURE").expect("ZOEN_PROVIDER_FIXTURE");
    let url = std::env::var("ZOEN_TEST_PG").expect("ZOEN_TEST_PG");
    let mut admin = PgConnection::connect(&url).await.unwrap();
    let schema = format!(
        "provider_oracle_{}",
        &hex::encode(Signer::generate().secret())[..16]
    );
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut admin)
        .await
        .unwrap();
    let path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(2)
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
    // Both migration/assertion failures are observed before the one owned schema
    // is dropped. No shared schema/table or service is reset by this test.
    let fixture_pool = pool.clone();
    let outcome = tokio::spawn(async move {
        sqlx::migrate!("./migrations")
            .run(&fixture_pool)
            .await
            .unwrap();
        let mut fixture = produce(fixture_pool.clone()).await;
        // Keep the original 20-case oracle intact. The additive Android cases
        // exercise the migrated CHECK, persisted client parser, real RSA token,
        // atomic proof consumption and ordinary signed device binding.
        fixture["android"] = android_scenarios(&fixture_pool).await;
        fixture
    })
    .await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&mut admin)
        .await
        .unwrap();
    let fixture = outcome.expect("provider SQL oracle failed after isolated schema cleanup");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &fixture).unwrap();
    file.write_all(b"\n").unwrap();
}
