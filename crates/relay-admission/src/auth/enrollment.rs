//! Provider enrollment policy. Hosts own entropy, configuration, HTTPS and durable
//! transactions. Facts are trusted reads for the exact lookup returned by this
//! module; no provider assertion alone authorizes enrollment of an existing account.

use super::Provider;
use protocol::provider_auth::{provider_bind_message, ProviderBindRequest, ProviderBindResponse};
use serde::{Deserialize, Serialize};
use types::IdentityKind;

pub const CHALLENGE_LIFETIME_MS: i64 = 300_000;
pub const PROOF_LIFETIME_MS: i64 = 300_000;
pub const CHALLENGE_CLEANUP_DELAY_MS: i64 = 600_000;
pub const CHALLENGE_CLEANUP_BATCH: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub status: u16,
    pub code: &'static str,
}
impl Failure {
    const fn new(status: u16, code: &'static str) -> Self {
        Self { status, code }
    }
    pub const fn unavailable() -> Self {
        Self::new(503, "authentication_unavailable")
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Client {
    Ios,
    Android,
    Web,
}
impl Client {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
            Self::Web => "web",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Audience {
    pub id: String,
    pub presenter: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub provider: Provider,
    pub client: Client,
    pub audience: String,
    pub presenter: Option<String>,
    pub origin: Option<String>,
    pub nonce: String,
    pub state: String,
    #[serde(with = "decimal_i64")]
    pub created_ms: i64,
    #[serde(with = "decimal_i64")]
    pub expires_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeRequest {
    pub provider: Provider,
    pub client: Client,
}

/// Deliberately has no Debug implementation: assertions must not enter logs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    pub challenge_id: String,
    pub id_token: String,
    pub state: String,
    pub authorization_code: Option<String>,
}

pub fn valid_audience_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_whitespace)
}

pub fn require_audience(audience: Option<&Audience>) -> Result<&Audience, Failure> {
    audience.ok_or(Failure::new(503, "provider_not_configured"))
}

pub fn require_provider_configured(configured: bool) -> Result<(), Failure> {
    if !configured {
        return Err(Failure::new(503, "provider_not_configured"));
    }
    Ok(())
}

/// Exact comparison against already validated, trusted configuration. URL parsing
/// and environment access remain host I/O; no caller-supplied allowlist is trusted.
pub fn origin<'a>(
    supplied: Option<&str>,
    client: Option<Client>,
    allowed: impl IntoIterator<Item = &'a str>,
) -> Result<Option<String>, Failure> {
    match supplied {
        Some(value) if allowed.into_iter().any(|origin| origin == value) => {
            Ok(Some(value.to_string()))
        }
        Some(_) => Err(Failure::new(403, "origin_refused")),
        None if client == Some(Client::Web) => Err(Failure::new(403, "origin_required")),
        None => Ok(None),
    }
}

pub fn token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn token_hash(value: &str) -> String {
    event_log::sha256_hex(value.as_bytes())
}

pub fn proof_hash(value: &str) -> Result<String, Failure> {
    if !token(value) {
        return Err(Failure::new(400, "invalid_authentication"));
    }
    Ok(token_hash(value))
}

pub fn validate_verify_request(request: &VerifyRequest) -> Result<(), Failure> {
    if !token(&request.challenge_id)
        || !token(&request.state)
        || request
            .authorization_code
            .as_ref()
            .is_some_and(|code| code.len() > 4096)
    {
        return Err(Failure::new(400, "invalid_authentication"));
    }
    Ok(())
}

pub fn validate_challenge(
    challenge: &Challenge,
    state: &str,
    supplied_origin: Option<&str>,
    current_audience: Option<&Audience>,
    now_ms: i64,
) -> Result<(), Failure> {
    if challenge.origin.as_deref() != supplied_origin
        || challenge.state != state
        || challenge.expires_ms <= now_ms
        || !current_audience.is_some_and(|audience| {
            audience.id == challenge.audience && audience.presenter == challenge.presenter
        })
    {
        return Err(Failure::new(401, "invalid_authentication"));
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BindLookup {
    pub token_hash: String,
    pub identity: String,
    pub device: String,
    pub handle: String,
}

/// Constructible only by strict signature/profile validation, never from JSON.
pub struct PreparedBinding {
    lookup: BindLookup,
    fingerprint: String,
}
impl PreparedBinding {
    pub fn lookup(&self) -> &BindLookup {
        &self.lookup
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

pub fn prepare_bind(request: &ProviderBindRequest) -> Result<PreparedBinding, Failure> {
    let profile = &request.profile;
    let handle =
        protocol::normalize_handle(&profile.handle).ok_or(Failure::new(400, "invalid_handle"))?;
    if !token(&request.proof_token)
        || profile.kind != IdentityKind::Person
        || profile.owner.is_some()
        || profile.owner_proof.is_some()
        || !profile.name.is_empty()
        || !profile.bio.is_empty()
        || profile.handle != handle
        || event_log::parse_device_key(&profile.id).is_none()
        || event_log::parse_device_key(&request.device).is_none()
        || !event_log::verify_sig(
            &profile.id,
            &event_log::device_cert_message(&request.device),
            &request.certificate,
        )
    {
        return Err(Failure::new(401, "invalid_binding_authorization"));
    }
    let payload = provider_bind_message(
        &request.proof_token,
        profile,
        &request.device,
        &request.certificate,
    )
    .map_err(|_| Failure::unavailable())?;
    if !event_log::verify_sig(&request.device, &payload, &request.signature) {
        return Err(Failure::new(401, "invalid_binding_authorization"));
    }
    Ok(PreparedBinding {
        lookup: BindLookup {
            token_hash: token_hash(&request.proof_token),
            identity: profile.id.clone(),
            device: request.device.clone(),
            handle,
        },
        fingerprint: event_log::sha256_hex(&payload),
    })
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub token_hash: String,
    pub provider: String,
    pub issuer: String,
    pub subject: String,
    pub origin: Option<String>,
    #[serde(with = "decimal_i64")]
    pub expires_ms: i64,
    pub consumed: bool,
    pub bound_fingerprint: Option<String>,
    pub bound_identity: Option<String>,
    pub bound_device: Option<String>,
    pub bound_handle: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofUse {
    Retry,
    Fresh,
}

/// The native SQL transaction calls these staged helpers under its row/advisory locks.
pub fn admit_proof(
    prepared: &PreparedBinding,
    proof: Option<&Proof>,
    supplied_origin: Option<&str>,
    now_ms: i64,
) -> Result<ProofUse, Failure> {
    let proof = proof.ok_or(Failure::new(401, "proof_expired"))?;
    if proof.token_hash != prepared.lookup.token_hash {
        return Err(Failure::unavailable());
    }
    if proof.origin.as_deref() != supplied_origin {
        return Err(Failure::new(403, "origin_refused"));
    }
    if proof.consumed {
        if proof.bound_fingerprint.as_deref() != Some(prepared.fingerprint())
            || proof.bound_identity.as_deref() != Some(&prepared.lookup.identity)
            || proof.bound_device.as_deref() != Some(&prepared.lookup.device)
        {
            return Err(Failure::new(409, "proof_already_used"));
        }
        return Ok(ProofUse::Retry);
    }
    if proof.expires_ms <= now_ms {
        return Err(Failure::new(401, "proof_expired"));
    }
    Ok(ProofUse::Fresh)
}

pub fn finish_retry(
    prepared: &PreparedBinding,
    proof: &Proof,
    active: bool,
) -> Result<ProviderBindResponse, Failure> {
    if !active {
        return Err(Failure::new(403, "device_revoked"));
    }
    let handle = proof
        .bound_handle
        .clone()
        .ok_or(Failure::new(409, "proof_already_used"))?;
    Ok(ProviderBindResponse {
        identity_id: prepared.lookup.identity.clone(),
        handle,
    })
}

pub fn check_provider_binding(existing: Option<&str>, identity: &str) -> Result<(), Failure> {
    if existing.is_some_and(|value| value != identity) {
        return Err(Failure::new(409, "provider_already_bound"));
    }
    Ok(())
}

pub fn check_identity_provider(existing: Option<&str>, subject: &str) -> Result<(), Failure> {
    if existing.is_some_and(|value| value != subject) {
        return Err(Failure::new(409, "identity_provider_already_bound"));
    }
    Ok(())
}

pub fn check_registered_kind(kind: &str) -> Result<(), Failure> {
    if kind != "Person" {
        return Err(Failure::new(409, "identity_kind_refused"));
    }
    Ok(())
}

pub fn existing_identity_handle(stored_handle: &str, active: bool) -> Result<String, Failure> {
    if !active {
        return Err(Failure::new(403, "link_device_first"));
    }
    Ok(stored_handle.to_string())
}

pub fn registration_failure(reason: &str) -> Failure {
    if reason == "handle_taken" {
        Failure::new(409, "handle_taken")
    } else {
        Failure::new(409, "registration_refused")
    }
}

mod decimal_i64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
