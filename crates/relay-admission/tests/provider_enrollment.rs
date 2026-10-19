use event_log::Signer;
use protocol::provider_auth::{provider_bind_message, ProviderBindRequest};
use relay_admission::auth::{enrollment::*, Provider};
use serde_json::json;
use types::{Identity, IdentityKind};

fn request() -> ProviderBindRequest {
    let root = Signer::from_secret(&[10; 32]);
    let device = Signer::from_secret(&[11; 32]);
    let mut request = ProviderBindRequest {
        proof_token: "ab".repeat(32),
        profile: Identity {
            id: root.id(),
            kind: IdentityKind::Person,
            name: String::new(),
            handle: "test_person".into(),
            tint_hex: "#888888".into(),
            glyph: Some("star".into()),
            owner: None,
            bio: String::new(),
            owner_proof: None,
        },
        device: device.id(),
        certificate: root.sign(&event_log::device_cert_message(&device.id())),
        signature: String::new(),
    };
    sign(&mut request);
    request
}
fn sign(request: &mut ProviderBindRequest) {
    request.signature = Signer::from_secret(&[11; 32]).sign(
        &provider_bind_message(
            &request.proof_token,
            &request.profile,
            &request.device,
            &request.certificate,
        )
        .unwrap(),
    );
}
fn failure<T>(result: Result<T, Failure>, status: u16, code: &str) {
    let error = result.err().expect("expected provider policy refusal");
    assert_eq!((error.status, error.code), (status, code));
}
fn challenge() -> Challenge {
    Challenge {
        provider: Provider::Google,
        client: Client::Web,
        audience: "audience".into(),
        presenter: Some("presenter".into()),
        origin: Some("https://app.example".into()),
        nonce: "11".repeat(32),
        state: "22".repeat(32),
        created_ms: 1_000,
        expires_ms: 301_000,
    }
}

#[test]
fn invalid_handle_precedes_all_other_binding_authorization_errors() {
    let mut request = request();
    request.profile.handle = "!".into();
    request.proof_token = "bad".into();
    request.signature.clear();
    failure(prepare_bind(&request), 400, "invalid_handle");
}

#[test]
fn private_fields_and_handle_are_rejected_instead_of_normalized_before_signing() {
    for field in ["name", "bio", "owner", "owner_proof", "handle", "kind"] {
        let mut request = request();
        match field {
            "name" => request.profile.name = "private name".into(),
            "bio" => request.profile.bio = "private bio".into(),
            "owner" => request.profile.owner = Some("owner".into()),
            "owner_proof" => {
                request.profile.owner_proof = Some(Box::new(types::AgentOwnerProof {
                    device: "device".into(),
                    cert: "cert".into(),
                    signature: "signature".into(),
                }));
            }
            "handle" => request.profile.handle = "Test_Person".into(),
            "kind" => request.profile.kind = IdentityKind::Agent,
            _ => unreachable!(),
        }
        sign(&mut request);
        failure(prepare_bind(&request), 401, "invalid_binding_authorization");
    }
}

#[test]
fn proof_root_certificate_and_device_signature_are_all_required() {
    for field in ["proof", "root", "device", "certificate", "signature"] {
        let mut request = request();
        match field {
            "proof" => request.proof_token = request.proof_token.to_uppercase(),
            "root" => request.profile.id = "not-a-key".into(),
            "device" => request.device = "not-a-key".into(),
            "certificate" => request.certificate = "00".repeat(64),
            "signature" => request.signature = "00".repeat(64),
            _ => unreachable!(),
        }
        failure(prepare_bind(&request), 401, "invalid_binding_authorization");
    }
    assert_eq!(
        proof_hash(&"ab".repeat(32)).unwrap(),
        token_hash(&"ab".repeat(32))
    );
    failure(proof_hash(&"AB".repeat(32)), 400, "invalid_authentication");
}

fn proof(request: &ProviderBindRequest) -> Proof {
    let prepared = prepare_bind(request).unwrap();
    Proof {
        token_hash: prepared.lookup().token_hash.clone(),
        provider: "google".into(),
        issuer: "https://accounts.google.com".into(),
        subject: "synthetic-subject".into(),
        origin: None,
        expires_ms: 10_000,
        consumed: false,
        bound_fingerprint: None,
        bound_identity: None,
        bound_device: None,
        bound_handle: None,
    }
}
fn consumed(request: &ProviderBindRequest) -> Proof {
    let mut proof = proof(request);
    proof.consumed = true;
    proof.bound_fingerprint = Some(prepare_bind(request).unwrap().fingerprint().into());
    proof.bound_identity = Some(request.profile.id.clone());
    proof.bound_device = Some(request.device.clone());
    proof.bound_handle = Some("original_handle".into());
    proof
}
#[test]
fn native_signed_payload_fingerprint_is_exact_and_never_contains_the_raw_proof_token() {
    let request = request();
    let prepared = prepare_bind(&request).unwrap();
    let payload = provider_bind_message(
        &request.proof_token,
        &request.profile,
        &request.device,
        &request.certificate,
    )
    .unwrap();
    assert_eq!(prepared.fingerprint(), event_log::sha256_hex(&payload));
    assert_eq!(
        prepared.lookup().token_hash,
        token_hash(&request.proof_token)
    );
    let lookup = serde_json::to_string(prepared.lookup()).unwrap();
    assert!(!lookup.contains(&request.proof_token));
    let decoded: ProviderBindRequest =
        serde_json::from_str(&serde_json::to_string_pretty(&request).unwrap()).unwrap();
    assert_eq!(
        prepare_bind(&decoded).unwrap().fingerprint(),
        prepared.fingerprint()
    );
}
#[test]
fn native_proof_origin_precedes_retry_or_expiry_and_unknown_proof_remains_refused() {
    let request = request();
    let prepared = prepare_bind(&request).unwrap();
    let mut used = consumed(&request);
    used.bound_fingerprint = None;
    failure(
        admit_proof(
            &prepared,
            Some(&used),
            Some("https://other.example"),
            20_000,
        ),
        403,
        "origin_refused",
    );
    failure(
        admit_proof(&prepared, Some(&used), None, 20_000),
        409,
        "proof_already_used",
    );
    let fresh = proof(&request);
    failure(
        admit_proof(&prepared, Some(&fresh), None, 10_000),
        401,
        "proof_expired",
    );
    failure(admit_proof(&prepared, None, None, 1), 401, "proof_expired");
    assert_eq!(
        admit_proof(&prepared, Some(&fresh), None, 1).unwrap(),
        ProofUse::Fresh
    );
}
#[test]
fn native_consumed_retry_survives_expiry_but_requires_exact_payload_and_active_device() {
    let mut request = request();
    let used = consumed(&request);
    request.signature = request.signature.to_uppercase();
    let prepared = prepare_bind(&request).unwrap();
    assert_eq!(
        admit_proof(&prepared, Some(&used), None, 20_000).unwrap(),
        ProofUse::Retry
    );
    assert_eq!(
        finish_retry(&prepared, &used, true).unwrap().handle,
        "original_handle"
    );
    failure(finish_retry(&prepared, &used, false), 403, "device_revoked");
    request.profile.tint_hex = "#123456".into();
    sign(&mut request);
    failure(
        admit_proof(&prepare_bind(&request).unwrap(), Some(&used), None, 20_000),
        409,
        "proof_already_used",
    );
    for field in ["identity", "device", "fingerprint"] {
        let mut bad = used.clone();
        match field {
            "identity" => bad.bound_identity = Some("other".into()),
            "device" => bad.bound_device = Some("other".into()),
            _ => bad.bound_fingerprint = None,
        };
        failure(
            admit_proof(&prepared, Some(&bad), None, 20_000),
            409,
            "proof_already_used",
        );
    }
    let mut missing = used;
    missing.bound_handle = None;
    failure(
        finish_retry(&prepared, &missing, false),
        403,
        "device_revoked",
    );
    failure(
        finish_retry(&prepared, &missing, true),
        409,
        "proof_already_used",
    );
}
#[test]
fn native_identity_provider_and_existing_account_checks_do_not_enroll_or_merge_by_email() {
    assert!(check_provider_binding(Some("same"), "same").is_ok());
    failure(
        check_provider_binding(Some("other"), "same"),
        409,
        "provider_already_bound",
    );
    failure(
        check_identity_provider(Some("other"), "same"),
        409,
        "identity_provider_already_bound",
    );
    failure(check_registered_kind("Agent"), 409, "identity_kind_refused");
    failure(
        existing_identity_handle("stored", false),
        403,
        "link_device_first",
    );
    assert_eq!(existing_identity_handle("stored", true).unwrap(), "stored");
    assert_eq!(registration_failure("handle_taken").code, "handle_taken");
    assert_eq!(registration_failure("other").code, "registration_refused");
}
#[test]
fn native_http_origin_configuration_and_challenge_validation_are_exact() {
    let origin_allowed = ["https://app.example"];
    failure(require_audience(None), 503, "provider_not_configured");
    failure(
        require_provider_configured(false),
        503,
        "provider_not_configured",
    );
    failure(
        origin(None, Some(Client::Web), origin_allowed),
        403,
        "origin_required",
    );
    assert_eq!(
        origin(None, Some(Client::Ios), origin_allowed).unwrap(),
        None
    );
    assert_eq!(
        origin(
            Some("https://app.example"),
            Some(Client::Web),
            origin_allowed
        )
        .unwrap(),
        Some("https://app.example".into())
    );
    failure(
        origin(
            Some("https://app.example.evil"),
            Some(Client::Web),
            origin_allowed,
        ),
        403,
        "origin_refused",
    );
    let challenge = challenge();
    let audience = Audience {
        id: "audience".into(),
        presenter: Some("presenter".into()),
    };
    assert!(validate_challenge(
        &challenge,
        &challenge.state,
        challenge.origin.as_deref(),
        Some(&audience),
        1_001
    )
    .is_ok());
    for (state, origin, now) in [
        ("wrong", challenge.origin.as_deref(), 1_001),
        (challenge.state.as_str(), Some("https://other"), 1_001),
        (
            challenge.state.as_str(),
            challenge.origin.as_deref(),
            challenge.expires_ms,
        ),
    ] {
        failure(
            validate_challenge(&challenge, state, origin, Some(&audience), now),
            401,
            "invalid_authentication",
        );
    }
    failure(
        validate_challenge(
            &challenge,
            &challenge.state,
            challenge.origin.as_deref(),
            None,
            1_001,
        ),
        401,
        "invalid_authentication",
    );
    let wrong = Audience {
        id: "different".into(),
        presenter: audience.presenter.clone(),
    };
    failure(
        validate_challenge(
            &challenge,
            &challenge.state,
            challenge.origin.as_deref(),
            Some(&wrong),
            1_001,
        ),
        401,
        "invalid_authentication",
    );
    let mut wire =
        json!({"challenge_id":"11".repeat(32),"state":"22".repeat(32),"id_token":"opaque-secret"});
    let good: VerifyRequest = serde_json::from_value(wire.clone()).unwrap();
    assert!(validate_verify_request(&good).is_ok());
    wire["authorization_code"] = json!("x".repeat(4097));
    failure(
        validate_verify_request(&serde_json::from_value(wire).unwrap()),
        400,
        "invalid_authentication",
    );
}
#[test]
fn native_challenge_and_proof_serde_keep_exact_decimal_times() {
    let mut challenge = challenge();
    challenge.created_ms = 9_007_199_254_740_993;
    let encoded = serde_json::to_value(&challenge).unwrap();
    assert_eq!(encoded["created_ms"], "9007199254740993");
    assert_eq!(
        serde_json::from_value::<Challenge>(encoded)
            .unwrap()
            .created_ms,
        challenge.created_ms
    );
    let request = request();
    let mut proof = proof(&request);
    proof.expires_ms = i64::MAX;
    assert_eq!(
        serde_json::from_value::<Proof>(serde_json::to_value(proof).unwrap())
            .unwrap()
            .expires_ms,
        i64::MAX
    );
}
