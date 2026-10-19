use event_log::Signer;
use protocol::provider_auth::{provider_bind_message, ProviderBindRequest};
use relay_admission::auth::{enrollment::*, Provider};
use serde_json::{json, Value};
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

fn facts(request: &ProviderBindRequest) -> BindFacts {
    let prepared = prepare_bind(request).unwrap();
    BindFacts {
        lookup: prepared.lookup().clone(),
        proof: Some(Proof {
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
        }),
        provider_binding: None,
        identity_provider: None,
        identity: None,
        device: None,
        handle_owner: None,
    }
}

fn enroll(facts: &mut BindFacts, request: &ProviderBindRequest) {
    facts.identity = Some(RegisteredIdentity {
        identity: request.profile.id.clone(),
        kind: "Person".into(),
        handle: "stored_handle".into(),
    });
    facts.device = Some(DeviceFact {
        device: request.device.clone(),
        identity: request.profile.id.clone(),
        cert: request.certificate.clone(),
        revoked: false,
    });
}

fn consume(facts: &mut BindFacts, request: &ProviderBindRequest) {
    enroll(facts, request);
    let proof = facts.proof.as_mut().unwrap();
    proof.consumed = true;
    proof.bound_fingerprint = Some(prepare_bind(request).unwrap().fingerprint().into());
    proof.bound_identity = Some(request.profile.id.clone());
    proof.bound_device = Some(request.device.clone());
    proof.bound_handle = Some("original_handle".into());
}

fn failure<T>(result: Result<T, Failure>, status: u16, code: &str) {
    let error = result.err().expect("expected provider policy refusal");
    assert_eq!((error.status, error.code), (status, code));
}

#[test]
fn exact_native_payload_and_canonical_serialization_define_the_receipt() {
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
    let json = serde_json::to_string_pretty(&request).unwrap();
    let lookup: Value = serde_json::from_str(&bind_lookup_json(&json).unwrap()).unwrap();
    assert_eq!(lookup["handle"], "test_person");
    assert!(!lookup.to_string().contains(&request.proof_token));
    let decoded: ProviderBindRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(
        prepare_bind(&decoded).unwrap().fingerprint(),
        prepared.fingerprint()
    );
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

#[test]
fn new_person_plan_preserves_canonical_profile_and_enrollment_without_committing() {
    let request = request();
    let facts = facts(&request);
    let plan = decide_bind(&request, &facts, None, 1).unwrap();
    assert!(matches!(plan.action, BindAction::Register));
    assert_eq!(plan.response.identity_id, request.profile.id);
    assert_eq!(plan.response.handle, request.profile.handle);
    let registration = plan.registration.unwrap();
    assert_eq!(
        serde_json::to_value(registration.profile).unwrap(),
        serde_json::to_value(&request.profile).unwrap()
    );
    assert_eq!(registration.device, request.device);
    assert_eq!(registration.certificate, request.certificate);
    assert!(!facts.proof.unwrap().consumed);
}

#[test]
fn origin_mismatch_precedes_consumption_and_expiry() {
    let request = request();
    let mut facts = facts(&request);
    consume(&mut facts, &request);
    facts.proof.as_mut().unwrap().bound_fingerprint = None;
    failure(
        decide_bind(&request, &facts, Some("https://other.example"), 20_000),
        403,
        "origin_refused",
    );
    failure(
        decide_bind(&request, &facts, None, 20_000),
        409,
        "proof_already_used",
    );
}

#[test]
fn consumed_retry_survives_expiry_and_returns_original_handle_without_profile_writes() {
    let mut request = request();
    let mut facts = facts(&request);
    consume(&mut facts, &request);
    request.signature = request.signature.to_uppercase();
    let plan = decide_bind(&request, &facts, None, 20_000).unwrap();
    assert!(matches!(plan.action, BindAction::Retry));
    assert_eq!(plan.response.handle, "original_handle");
    assert!(plan.registration.is_none());
    assert_eq!(facts.identity.unwrap().handle, "stored_handle");
}

#[test]
fn consumed_retry_requires_exact_active_identity_device_and_certificate() {
    for field in ["missing", "identity", "certificate", "revoked"] {
        let request = request();
        let mut facts = facts(&request);
        consume(&mut facts, &request);
        match field {
            "missing" => facts.device = None,
            "identity" => facts.device.as_mut().unwrap().identity = "other".into(),
            "certificate" => {
                facts.device.as_mut().unwrap().cert = request.certificate.to_uppercase()
            }
            "revoked" => facts.device.as_mut().unwrap().revoked = true,
            _ => unreachable!(),
        }
        failure(
            decide_bind(&request, &facts, None, 20_000),
            403,
            "device_revoked",
        );
    }
}

#[test]
fn changed_payload_cannot_replay_but_signature_encoding_is_not_receipt_identity() {
    let mut request = request();
    let mut facts = facts(&request);
    consume(&mut facts, &request);
    request.profile.tint_hex = "#123456".into();
    sign(&mut request);
    failure(
        decide_bind(&request, &facts, None, 20_000),
        409,
        "proof_already_used",
    );
}

#[test]
fn retry_checks_device_before_incomplete_receipt_and_never_becomes_a_new_bind() {
    let request = request();
    let mut facts = facts(&request);
    consume(&mut facts, &request);
    facts.proof.as_mut().unwrap().bound_handle = None;
    facts.device.as_mut().unwrap().revoked = true;
    failure(
        decide_bind(&request, &facts, None, 20_000),
        403,
        "device_revoked",
    );
    facts.device.as_mut().unwrap().revoked = false;
    failure(
        decide_bind(&request, &facts, None, 20_000),
        409,
        "proof_already_used",
    );
}

#[test]
fn fresh_proof_expiry_precedes_binding_and_registered_identity_checks() {
    let request = request();
    let mut facts = facts(&request);
    enroll(&mut facts, &request);
    facts.identity.as_mut().unwrap().kind = "Agent".into();
    failure(
        decide_bind(&request, &facts, None, 10_000),
        401,
        "proof_expired",
    );
    facts.proof = None;
    failure(decide_bind(&request, &facts, None, 1), 401, "proof_expired");
}

#[test]
fn binding_conflicts_precede_kind_and_existing_accounts_never_enroll_or_update() {
    let request = request();
    let mut facts = facts(&request);
    enroll(&mut facts, &request);
    facts.identity.as_mut().unwrap().kind = "Agent".into();
    facts.provider_binding = Some(ProviderBinding {
        provider: "google".into(),
        issuer: "https://accounts.google.com".into(),
        subject: "synthetic-subject".into(),
        identity: "other".into(),
    });
    failure(
        decide_bind(&request, &facts, None, 1),
        409,
        "provider_already_bound",
    );
    facts.provider_binding = None;
    facts.identity_provider = Some(IdentityProvider {
        identity: request.profile.id.clone(),
        provider: "google".into(),
        subject: "other-subject".into(),
    });
    failure(
        decide_bind(&request, &facts, None, 1),
        409,
        "identity_provider_already_bound",
    );
    facts.identity_provider = None;
    failure(
        decide_bind(&request, &facts, None, 1),
        409,
        "identity_kind_refused",
    );
    facts.identity.as_mut().unwrap().kind = "Person".into();
    facts.device.as_mut().unwrap().revoked = true;
    failure(
        decide_bind(&request, &facts, None, 1),
        403,
        "link_device_first",
    );
    facts.device.as_mut().unwrap().revoked = false;
    let plan = decide_bind(&request, &facts, None, 1).unwrap();
    assert!(matches!(plan.action, BindAction::Existing));
    assert_eq!(plan.response.handle, "stored_handle");
    assert!(plan.registration.is_none());
}

#[test]
fn first_registration_handle_conflict_precedes_enrollment_and_revocation_is_never_revived() {
    let request = request();
    let mut facts = facts(&request);
    facts.device = Some(DeviceFact {
        device: request.device.clone(),
        identity: "other".into(),
        cert: request.certificate.clone(),
        revoked: true,
    });
    facts.handle_owner = Some("other".into());
    failure(decide_bind(&request, &facts, None, 1), 409, "handle_taken");
    facts.handle_owner = None;
    failure(
        decide_bind(&request, &facts, None, 1),
        409,
        "registration_refused",
    );
}

#[test]
fn trusted_projection_scope_is_bound_to_request_and_provider_subject() {
    for field in [
        "lookup",
        "proof",
        "device",
        "binding",
        "identity_provider",
        "identity",
    ] {
        let request = request();
        let mut facts = facts(&request);
        match field {
            "lookup" => facts.lookup.handle = "different".into(),
            "proof" => facts.proof.as_mut().unwrap().token_hash = "00".repeat(32),
            "device" => {
                enroll(&mut facts, &request);
                facts.device.as_mut().unwrap().device = "different".into();
            }
            "binding" => {
                facts.provider_binding = Some(ProviderBinding {
                    provider: "google".into(),
                    issuer: "https://accounts.google.com".into(),
                    subject: "different".into(),
                    identity: request.profile.id.clone(),
                })
            }
            "identity_provider" => {
                facts.identity_provider = Some(IdentityProvider {
                    identity: "different".into(),
                    provider: "google".into(),
                    subject: "synthetic-subject".into(),
                })
            }
            "identity" => {
                enroll(&mut facts, &request);
                facts.identity.as_mut().unwrap().identity = "different".into();
            }
            _ => unreachable!(),
        }
        failure(
            decide_bind(&request, &facts, None, 1),
            503,
            "authentication_unavailable",
        );
    }
}

fn settings() -> Value {
    json!({"audiences":[{"provider":"google","client":"web","audience":{"id":"audience","presenter":"presenter"}}],"origins":["https://app.example"]})
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
fn challenge_configuration_precedes_origin_and_web_origin_is_exact() {
    failure(
        challenge_policy_json(
            r#"{"provider":"google","client":"web"}"#,
            &json!({"settings":{"audiences":[],"origins":[]},"origin":"https://evil.example"})
                .to_string(),
        ),
        503,
        "provider_not_configured",
    );
    failure(
        challenge_policy_json(
            r#"{"provider":"google","client":"web"}"#,
            &json!({"settings":settings(),"origin":null}).to_string(),
        ),
        403,
        "origin_required",
    );
    failure(
        origin(
            Some("https://app.example/"),
            Some(Client::Web),
            ["https://app.example"],
        ),
        403,
        "origin_refused",
    );
    assert!(origin(None, Some(Client::Ios), ["https://app.example"])
        .unwrap()
        .is_none());
    assert!(require_provider_configured(true).is_ok());
    failure(
        require_provider_configured(false),
        503,
        "provider_not_configured",
    );
}

#[test]
fn android_configuration_is_explicit_and_never_inherits_ios_or_web_audiences() {
    let request = r#"{"provider":"google","client":"android"}"#;
    let without_android = json!({"settings":settings(),"origin":null});
    failure(
        challenge_policy_json(request, &without_android.to_string()),
        503,
        "provider_not_configured",
    );
    let mut configured = settings();
    configured["audiences"].as_array_mut().unwrap().push(json!({
        "provider":"google","client":"android","audience":{"id":"android-server","presenter":"android-client"}
    }));
    let context = json!({"settings":configured,"origin":null});
    let policy: Value =
        serde_json::from_str(&challenge_policy_json(request, &context.to_string()).unwrap())
            .unwrap();
    assert_eq!(
        policy,
        json!({"audience":{"id":"android-server","presenter":"android-client"},"origin":null})
    );
    failure(
        challenge_policy_json(
            r#"{"provider":"apple","client":"android"}"#,
            &context.to_string(),
        ),
        503,
        "provider_not_configured",
    );
    failure(
        challenge_policy_json(
            request,
            &json!({"settings":configured,"origin":"https://evil.example"}).to_string(),
        ),
        403,
        "origin_refused",
    );
    assert_eq!(Client::Android.name(), "android");
    configured["audiences"][1]["audience"]["presenter"] = Value::Null;
    failure(
        challenge_policy_json(
            request,
            &json!({"settings":configured,"origin":null}).to_string(),
        ),
        503,
        "provider_not_configured",
    );
}

#[test]
fn bind_http_config_and_origin_precede_crypto_without_requiring_the_proofs_pair() {
    let mut request = request();
    request.signature.clear();
    let disabled =
        json!({"settings":{"audiences":[],"origins":[]},"origin":"https://evil.example"});
    failure(
        bind_policy_json(
            &serde_json::to_string(&request).unwrap(),
            &disabled.to_string(),
        ),
        503,
        "provider_not_configured",
    );
    let allowed = json!({"settings":settings(),"origin":"https://evil.example"});
    failure(
        bind_policy_json(
            &serde_json::to_string(&request).unwrap(),
            &allowed.to_string(),
        ),
        403,
        "origin_refused",
    );
    let native = json!({"settings":settings(),"origin":null});
    failure(
        bind_policy_json(
            &serde_json::to_string(&request).unwrap(),
            &native.to_string(),
        ),
        401,
        "invalid_binding_authorization",
    );
    sign(&mut request);
    assert!(bind_policy_json(
        &serde_json::to_string(&request).unwrap(),
        &native.to_string()
    )
    .is_ok());
}

#[test]
fn verification_request_challenge_origin_and_current_settings_order_stays_native() {
    let request = json!({"challenge_id":"33".repeat(32),"id_token":"not-verified-in-this-policy-gate","state":"22".repeat(32)});
    let context = json!({"settings":settings(),"origin":"https://app.example","challenge":challenge(),"now_ms":"1001"});
    assert!(verify_policy_json(&request.to_string(), &context.to_string()).is_ok());
    let mut bad_request = request.clone();
    bad_request["state"] = json!("bad");
    let mut missing = context.clone();
    missing["challenge"] = Value::Null;
    missing["origin"] = Value::Null;
    failure(
        verify_policy_json(&bad_request.to_string(), &missing.to_string()),
        400,
        "invalid_authentication",
    );
    failure(
        verify_policy_json(&request.to_string(), &missing.to_string()),
        401,
        "challenge_expired",
    );
    for field in [
        "origin",
        "state",
        "expiry",
        "audience",
        "presenter",
        "disabled",
    ] {
        let mut context = context.clone();
        match field {
            "origin" => context["origin"] = json!("https://evil.example"),
            "state" => context["challenge"]["state"] = json!("44".repeat(32)),
            "expiry" => context["now_ms"] = json!("301000"),
            "audience" => {
                context["settings"]["audiences"][0]["audience"]["id"] = json!("new-audience")
            }
            "presenter" => {
                context["settings"]["audiences"][0]["audience"]["presenter"] =
                    json!("new-presenter")
            }
            "disabled" => context["settings"]["audiences"] = json!([]),
            _ => unreachable!(),
        }
        let (status, code) = if field == "origin" {
            (403, "origin_refused")
        } else {
            (401, "invalid_authentication")
        };
        failure(
            verify_policy_json(&request.to_string(), &context.to_string()),
            status,
            code,
        );
    }
}

#[test]
fn portable_boundaries_keep_exact_decimal_times_and_hide_proof_material() {
    let request = request();
    let mut facts = facts(&request);
    facts.proof.as_mut().unwrap().expires_ms = i64::MAX;
    let context = json!({"facts":facts,"origin":null,"now_ms":"9007199254740993"});
    assert_eq!(
        context["facts"]["proof"]["expires_ms"],
        i64::MAX.to_string()
    );
    let output = bind_decision_json(
        &serde_json::to_string(&request).unwrap(),
        &context.to_string(),
    )
    .unwrap();
    assert!(!output.contains(&request.proof_token));
    assert_eq!(
        serde_json::from_str::<Value>(&output).unwrap()["action"],
        "register"
    );
    let mut invalid = context;
    invalid["now_ms"] = json!("1000.5");
    failure(
        bind_decision_json(
            &serde_json::to_string(&request).unwrap(),
            &invalid.to_string(),
        ),
        503,
        "authentication_unavailable",
    );
    failure(
        bind_lookup_json(&format!(
            "{{\"unknown\":0,{}",
            &serde_json::to_string(&request).unwrap()[1..]
        )),
        400,
        "invalid_authentication",
    );
}
