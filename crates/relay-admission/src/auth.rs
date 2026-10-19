//! Bounded provider assertion verification for native HTTP authentication.
//! Provider subjects never merge identities by email. Hosts supply a previously
//! persisted challenge, trusted provider keys and the current trusted clock.
pub mod enrollment;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Apple,
    Google,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Apple => "apple",
            Self::Google => "google",
        }
    }
}

pub struct VerificationContext<'a> {
    pub provider: Provider,
    pub audience: &'a str,
    pub presenter: Option<&'a str>,
    pub nonce: &'a str,
    pub created_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAssertion;
impl std::fmt::Display for InvalidAssertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid provider assertion")
    }
}
impl std::error::Error for InvalidAssertion {}

#[derive(Clone, Deserialize, Serialize)]
pub struct Jwk {
    pub kid: String,
    kty: String,
    #[serde(default)]
    alg: Option<String>,
    #[serde(rename = "use", default)]
    use_: Option<String>,
    #[serde(default)]
    key_ops: Option<Vec<String>>,
    n: String,
    e: String,
}
#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    alg: String,
    kid: String,
    typ: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}
impl Audience {
    fn includes(&self, audience: &str) -> bool {
        match self {
            Self::One(value) => value == audience,
            Self::Many(values) => values.len() <= 8 && values.iter().any(|value| value == audience),
        }
    }
    fn multiple(&self) -> bool {
        matches!(self,Self::Many(values) if values.len()>1)
    }
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    aud: Audience,
    exp: i64,
    iat: i64,
    nonce: String,
    azp: Option<String>,
    name: Option<String>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Verified {
    pub issuer: String,
    pub subject: String,
    pub name: Option<String>,
}

/// Parses only the bounded JWT header. Provider URLs always come from trusted config.
pub fn key_id(token: &str) -> Result<String, InvalidAssertion> {
    let (header, _, _) = segments(token)?;
    let parsed: Header =
        serde_json::from_slice(&decode(header, 2048)?).map_err(|_| InvalidAssertion)?;
    if parsed.alg != "RS256"
        || parsed.kid.is_empty()
        || parsed.kid.len() > 128
        || parsed.typ.as_ref().is_some_and(|value| value != "JWT")
    {
        return Err(InvalidAssertion);
    }
    Ok(parsed.kid)
}

/// Shared bounded JWKS parsing; caching and HTTPS fetching remain host responsibilities.
pub fn key_set(bytes: &[u8]) -> Result<Vec<Jwk>, InvalidAssertion> {
    if bytes.len() > 65_536 {
        return Err(InvalidAssertion);
    }
    let keys: Jwks = serde_json::from_slice(bytes).map_err(|_| InvalidAssertion)?;
    let mut ids = std::collections::BTreeSet::new();
    if keys.keys.is_empty()
        || keys.keys.len() > 64
        || keys.keys.iter().any(|key| !ids.insert(key.kid.clone()))
    {
        return Err(InvalidAssertion);
    }
    Ok(keys.keys)
}

fn segments(token: &str) -> Result<(&str, &str, &str), InvalidAssertion> {
    if token.len() > 16_384 {
        return Err(InvalidAssertion);
    }
    let mut parts = token.split('.');
    let result = (
        parts.next().ok_or(InvalidAssertion)?,
        parts.next().ok_or(InvalidAssertion)?,
        parts.next().ok_or(InvalidAssertion)?,
    );
    if parts.next().is_some() || result.0.is_empty() || result.1.is_empty() || result.2.is_empty() {
        return Err(InvalidAssertion);
    }
    Ok(result)
}
fn decode(value: &str, max: usize) -> Result<Vec<u8>, InvalidAssertion> {
    if value.len() > max * 2 {
        return Err(InvalidAssertion);
    }
    let result = B64.decode(value).map_err(|_| InvalidAssertion)?;
    if result.len() > max {
        return Err(InvalidAssertion);
    }
    Ok(result)
}
pub fn verify_with_key(
    token: &str,
    key: &Jwk,
    challenge: &VerificationContext<'_>,
    now: i64,
) -> Result<Verified, InvalidAssertion> {
    let selected_id = key_id(token)?;
    if selected_id != key.kid {
        return Err(InvalidAssertion);
    }
    let (header, payload, sig) = segments(token)?;
    let header: Header =
        serde_json::from_slice(&decode(header, 2048)?).map_err(|_| InvalidAssertion)?;
    if header.alg != "RS256"
        || header.kid != key.kid
        || header.typ.as_ref().is_some_and(|value| value != "JWT")
        || key.kty != "RSA"
        || key.alg.as_ref().is_some_and(|alg| alg != "RS256")
        || key.use_.as_ref().is_some_and(|use_| use_ != "sig")
        || key
            .key_ops
            .as_ref()
            .is_some_and(|ops| !ops.iter().any(|op| op == "verify"))
    {
        return Err(InvalidAssertion);
    }
    let n = decode(&key.n, 512)?;
    let e = decode(&key.e, 8)?;
    ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            &token.as_bytes()[..token.rfind('.').ok_or(InvalidAssertion)?],
            &decode(sig, 512)?,
        )
        .map_err(|_| InvalidAssertion)?;
    let claims: Claims =
        serde_json::from_slice(&decode(payload, 12_288)?).map_err(|_| InvalidAssertion)?;
    let issuer = match challenge.provider {
        Provider::Apple if claims.iss == "https://appleid.apple.com" => "https://appleid.apple.com",
        Provider::Google
            if ["accounts.google.com", "https://accounts.google.com"]
                .contains(&claims.iss.as_str()) =>
        {
            "https://accounts.google.com"
        }
        _ => return Err(InvalidAssertion),
    };
    let now = now / 1000;
    if claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.sub.chars().any(char::is_control)
        || !claims.aud.includes(challenge.audience)
        || claims.nonce != challenge.nonce
        || claims.exp <= now
        || claims.iat > now + 30
        || claims.iat < challenge.created_ms / 1000 - 60
        || claims.exp <= claims.iat
        || claims.exp.saturating_sub(claims.iat) > 7200
        || claims.aud.multiple() && claims.azp.is_none()
        || challenge.presenter.is_some() && claims.azp.as_deref() != challenge.presenter
        || claims
            .azp
            .as_deref()
            .is_some_and(|azp| azp != challenge.presenter.unwrap_or(challenge.audience))
    {
        return Err(InvalidAssertion);
    }
    Ok(Verified {
        issuer: issuer.into(),
        subject: claims.sub,
        name: claims
            .name
            .map(|name| {
                name.chars()
                    .filter(|character| !character.is_control())
                    .take(100)
                    .collect()
            })
            .filter(|name: &String| !name.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{
        rand::SystemRandom,
        signature::{RsaKeyPair, RSA_PKCS1_SHA256},
    };
    use serde_json::{json, Value};
    fn challenge(provider: Provider) -> VerificationContext<'static> {
        VerificationContext {
            provider,
            audience: "audience",
            presenter: Some("ios-client"),
            nonce: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            created_ms: 1_000_000,
        }
    }
    fn key() -> Jwk {
        serde_json::from_str(include_str!("auth/fixtures/rsa-test-only.jwk.json")).unwrap()
    }
    fn claims() -> Value {
        json!({"iss":"https://accounts.google.com","sub":"provider-subject","aud":"audience","azp":"ios-client","iat":1000,"exp":1600,"nonce":"aa".repeat(32),"name":"Display hint"})
    }
    fn signed(header: Value, claims: Value) -> String {
        // Disposable synthetic key: never configured with either real provider.
        let key =
            RsaKeyPair::from_pkcs8(include_bytes!("auth/fixtures/rsa-test-only.pk8")).unwrap();
        let input = format!(
            "{}.{}",
            B64.encode(serde_json::to_vec(&header).unwrap()),
            B64.encode(serde_json::to_vec(&claims).unwrap())
        );
        let mut signature = vec![0; key.public().modulus_len()];
        key.sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            input.as_bytes(),
            &mut signature,
        )
        .unwrap();
        format!("{input}.{}", B64.encode(signature))
    }
    fn token(claims: Value) -> String {
        signed(json!({"alg":"RS256","kid":"test-key","typ":"JWT"}), claims)
    }

    #[test]
    fn real_rsa_assertions_require_nonce_audience_presenter_issuer_and_bounded_times() {
        let challenge = challenge(Provider::Google);
        let valid = verify_with_key(&token(claims()), &key(), &challenge, 1_000_000).unwrap();
        assert_eq!(valid.subject, "provider-subject");
        for (field, value) in [
            ("nonce", json!("wrong")),
            ("aud", json!("different")),
            ("azp", json!("attacker-client")),
            ("iss", json!("https://evil.example")),
            ("exp", json!(999)),
            ("iat", json!(1031)),
            ("iat", json!(900)),
            ("sub", json!("")),
        ] {
            let mut claims = claims();
            claims[field] = value;
            assert!(
                verify_with_key(&token(claims), &key(), &challenge, 1_000_000).is_err(),
                "accepted wrong {field}"
            );
        }
        let mut multiple = claims();
        multiple["aud"] = json!(["audience", "other"]);
        multiple.as_object_mut().unwrap().remove("azp");
        assert!(verify_with_key(&token(multiple), &key(), &challenge, 1_000_000).is_err());
        let mut apple = claims();
        apple["iss"] = json!("https://appleid.apple.com");
        apple.as_object_mut().unwrap().remove("azp");
        let mut apple_challenge = challenge;
        apple_challenge.provider = Provider::Apple;
        apple_challenge.presenter = None;
        assert!(verify_with_key(&token(apple), &key(), &apple_challenge, 1_000_000).is_ok());
    }
    #[test]
    fn android_google_assertions_require_the_android_presenter_even_with_a_shared_server_audience()
    {
        let mut android = challenge(Provider::Google);
        android.presenter = Some("android-client");
        let mut android_claims = claims();
        android_claims["azp"] = json!("android-client");
        let assertion = token(android_claims.clone());
        assert!(verify_with_key(&assertion, &key(), &android, 1_000_000).is_ok());
        // Both apps request the same backend audience. The authorized presenter
        // still separates Android credentials from iOS and browser credentials.
        assert!(
            verify_with_key(&assertion, &key(), &challenge(Provider::Google), 1_000_000).is_err()
        );
        for presenter in [json!("ios-client"), json!("web-client"), Value::Null] {
            android_claims["azp"] = presenter;
            assert!(
                verify_with_key(&token(android_claims.clone()), &key(), &android, 1_000_000)
                    .is_err()
            );
        }
        android_claims["azp"] = json!("android-client");
        android_claims["aud"] = json!("android-client");
        assert!(verify_with_key(&token(android_claims), &key(), &android, 1_000_000).is_err());
    }
    #[test]
    fn jwt_algorithms_header_urls_key_uses_and_signature_tampering_are_refused() {
        let challenge = challenge(Provider::Google);
        for header in [
            json!({"alg":"HS256","kid":"test-key"}),
            json!({"alg":"none","kid":"test-key"}),
            json!({"alg":"RS256","kid":"test-key","jku":"https://evil.example/keys"}),
            json!({"alg":"RS256","kid":"test-key","crit":["b64"],"b64":false}),
        ] {
            assert!(
                verify_with_key(&signed(header, claims()), &key(), &challenge, 1_000_000).is_err()
            );
        }
        let mut wrong = key();
        wrong.use_ = Some("enc".into());
        assert!(verify_with_key(&token(claims()), &wrong, &challenge, 1_000_000).is_err());
        let mut encoded = token(claims()).into_bytes();
        let last = encoded.len() - 2;
        encoded[last] = if encoded[last] == b'A' { b'B' } else { b'A' };
        assert!(verify_with_key(
            std::str::from_utf8(&encoded).unwrap(),
            &key(),
            &challenge,
            1_000_000
        )
        .is_err());
    }

    #[test]
    fn native_verifier_preserves_subject_and_sanitizes_only_the_display_hint() {
        let context = challenge(Provider::Google);
        let mut first = claims();
        first["email"] = json!("shared@example.invalid");
        first["name"] = json!(format!("\n{}\u{7}", "é".repeat(110)));
        let verified = verify_with_key(&token(first.clone()), &key(), &context, 1_000_000).unwrap();
        assert_eq!(
            verified,
            Verified {
                issuer: "https://accounts.google.com".into(),
                subject: "provider-subject".into(),
                name: Some("é".repeat(100))
            }
        );
        first["sub"] = json!("another-provider-subject");
        let second = verify_with_key(&token(first), &key(), &context, 1_000_000).unwrap();
        assert_eq!(second.subject, "another-provider-subject");
        assert!(serde_json::to_value(second).unwrap().get("email").is_none());
    }

    #[test]
    fn key_selection_is_bounded_and_never_follows_token_supplied_urls() {
        let key: Value =
            serde_json::from_str(include_str!("auth/fixtures/rsa-test-only.jwk.json")).unwrap();
        assert_eq!(
            key_set(
                serde_json::to_string(&json!({"keys":[key.clone()]}))
                    .unwrap()
                    .as_bytes()
            )
            .unwrap()
            .len(),
            1
        );
        for keys in [
            json!([]),
            json!([key.clone(), key.clone()]),
            json!(vec![key; 65]),
        ] {
            assert!(key_set(
                serde_json::to_string(&json!({"keys":keys}))
                    .unwrap()
                    .as_bytes()
            )
            .is_err());
        }
        assert!(key_set(&vec![b' '; 65_537]).is_err());
        for kid in [String::new(), "a".repeat(129)] {
            assert!(key_id(&signed(json!({"alg":"RS256","kid":kid}), claims())).is_err());
        }
        assert!(key_id(&"a".repeat(16_385)).is_err());
        assert!(key_id("a.b.c.extra").is_err());
    }
}
