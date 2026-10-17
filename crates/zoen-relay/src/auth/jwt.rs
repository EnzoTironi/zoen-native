use super::{Challenge, Provider};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Deserialize)]
pub(super) struct Jwk {
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
pub(super) struct Verified {
    pub issuer: String,
    pub subject: String,
    pub name: Option<String>,
}
#[derive(Default)]
struct Cache {
    keys: Vec<Jwk>,
    expires: Option<Instant>,
    refreshed: Option<Instant>,
}
pub(super) struct Verifier {
    http: reqwest::Client,
    caches: BTreeMap<Provider, Arc<Mutex<Cache>>>,
}
impl Verifier {
    pub fn new() -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .https_only(true)
            .build()?;
        Ok(Self {
            http,
            caches: [Provider::Apple, Provider::Google]
                .into_iter()
                .map(|provider| (provider, Arc::new(Mutex::new(Cache::default()))))
                .collect(),
        })
    }
    pub async fn verify(
        &self,
        token: &str,
        challenge: &Challenge,
        now: i64,
    ) -> Result<Verified, ()> {
        let (header, _, _) = segments(token)?;
        let parsed: Header = serde_json::from_slice(&decode(header, 2048)?).map_err(|_| ())?;
        if parsed.alg != "RS256"
            || parsed.kid.is_empty()
            || parsed.kid.len() > 128
            || parsed.typ.as_ref().is_some_and(|value| value != "JWT")
        {
            return Err(());
        }
        let mut cache = self.caches[&challenge.provider].lock().await;
        let now_cache = Instant::now();
        let retained = cache.keys.iter().find(|key| key.kid == parsed.kid).cloned();
        if cache.expires.is_none_or(|at| at <= now_cache) || retained.is_none() {
            if cache
                .refreshed
                .is_some_and(|at| now_cache.duration_since(at) < Duration::from_secs(10))
            {
                return Err(());
            }
            cache.refreshed = Some(now_cache);
            let url = match challenge.provider {
                Provider::Apple => "https://appleid.apple.com/auth/keys",
                Provider::Google => "https://www.googleapis.com/oauth2/v3/certs",
            };
            let mut response = self.http.get(url).send().await.map_err(|_| ())?;
            if !response.status().is_success()
                || response.content_length().is_some_and(|size| size > 65536)
            {
                return Err(());
            }
            let ttl = response
                .headers()
                .get(reqwest::header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    value
                        .split(',')
                        .find_map(|part| part.trim().strip_prefix("max-age=")?.parse::<u64>().ok())
                })
                .unwrap_or(300)
                .clamp(60, 3600);
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
                if body.len() + chunk.len() > 65536 {
                    return Err(());
                }
                body.extend_from_slice(&chunk);
            }
            let keys: Jwks = serde_json::from_slice(&body).map_err(|_| ())?;
            if keys.keys.is_empty() || keys.keys.len() > 64 {
                return Err(());
            }
            let mut ids = std::collections::BTreeSet::new();
            if keys.keys.iter().any(|key| !ids.insert(key.kid.clone())) {
                return Err(());
            }
            cache.keys = keys.keys;
            cache.expires = Some(now_cache + Duration::from_secs(ttl));
        }
        let key = cache
            .keys
            .iter()
            .find(|key| key.kid == parsed.kid)
            .cloned()
            .ok_or(())?;
        drop(cache);
        verify_with_key(token, &key, challenge, now)
    }
}
fn segments(token: &str) -> Result<(&str, &str, &str), ()> {
    if token.len() > 16_384 {
        return Err(());
    }
    let mut parts = token.split('.');
    let result = (
        parts.next().ok_or(())?,
        parts.next().ok_or(())?,
        parts.next().ok_or(())?,
    );
    if parts.next().is_some() || result.0.is_empty() || result.1.is_empty() || result.2.is_empty() {
        return Err(());
    }
    Ok(result)
}
fn decode(value: &str, max: usize) -> Result<Vec<u8>, ()> {
    if value.len() > max * 2 {
        return Err(());
    }
    let result = B64.decode(value).map_err(|_| ())?;
    if result.len() > max {
        return Err(());
    }
    Ok(result)
}
pub(super) fn verify_with_key(
    token: &str,
    key: &Jwk,
    challenge: &Challenge,
    now: i64,
) -> Result<Verified, ()> {
    let (header, payload, sig) = segments(token)?;
    let header: Header = serde_json::from_slice(&decode(header, 2048)?).map_err(|_| ())?;
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
        return Err(());
    }
    let n = decode(&key.n, 512)?;
    let e = decode(&key.e, 8)?;
    ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            &token.as_bytes()[..token.rfind('.').ok_or(())?],
            &decode(sig, 512)?,
        )
        .map_err(|_| ())?;
    let claims: Claims = serde_json::from_slice(&decode(payload, 12_288)?).map_err(|_| ())?;
    let issuer = match challenge.provider {
        Provider::Apple if claims.iss == "https://appleid.apple.com" => "https://appleid.apple.com",
        Provider::Google
            if ["accounts.google.com", "https://accounts.google.com"]
                .contains(&claims.iss.as_str()) =>
        {
            "https://accounts.google.com"
        }
        _ => return Err(()),
    };
    let now = now / 1000;
    if claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.sub.chars().any(char::is_control)
        || !claims.aud.includes(&challenge.audience)
        || claims.nonce != challenge.nonce
        || claims.exp <= now
        || claims.iat > now + 30
        || claims.iat < challenge.created_ms / 1000 - 60
        || claims.exp <= claims.iat
        || claims.exp.saturating_sub(claims.iat) > 7200
        || claims.aud.multiple() && claims.azp.is_none()
        || challenge.presenter.is_some() && claims.azp.as_deref() != challenge.presenter.as_deref()
        || claims.azp.as_deref().is_some_and(|azp| {
            azp != challenge
                .presenter
                .as_deref()
                .unwrap_or(&challenge.audience)
        })
    {
        return Err(());
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
    fn challenge(provider: Provider) -> Challenge {
        Challenge {
            provider,
            client: super::super::Client::Ios,
            audience: "audience".into(),
            presenter: Some("ios-client".into()),
            origin: None,
            nonce: "aa".repeat(32),
            state: "bb".repeat(32),
            created_ms: 1_000_000,
            expires_ms: 1_300_000,
        }
    }
    fn key() -> Jwk {
        serde_json::from_str(include_str!("fixtures/rsa-test-only.jwk.json")).unwrap()
    }
    fn claims() -> Value {
        json!({"iss":"https://accounts.google.com","sub":"provider-subject","aud":"audience","azp":"ios-client","iat":1000,"exp":1600,"nonce":"aa".repeat(32),"name":"Display hint"})
    }
    fn signed(header: Value, claims: Value) -> String {
        // Disposable synthetic key: never configured with either real provider.
        let key = RsaKeyPair::from_pkcs8(include_bytes!("fixtures/rsa-test-only.pk8")).unwrap();
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
        let mut apple_challenge = challenge.clone();
        apple_challenge.provider = Provider::Apple;
        apple_challenge.presenter = None;
        assert!(verify_with_key(&token(apple), &key(), &apple_challenge, 1_000_000).is_ok());
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
    #[tokio::test]
    async fn cached_keys_verify_without_network_and_unknown_kids_cannot_force_a_refresh_storm() {
        let verifier = Verifier::new().unwrap();
        let mut cache = verifier.caches[&Provider::Google].lock().await;
        cache.keys = vec![key()];
        cache.expires = Some(Instant::now() + Duration::from_secs(300));
        cache.refreshed = Some(Instant::now());
        drop(cache);
        assert!(verifier
            .verify(&token(claims()), &challenge(Provider::Google), 1_000_000)
            .await
            .is_ok());
        let unknown = signed(json!({"alg":"RS256","kid":"unknown"}), claims());
        assert!(verifier
            .verify(&unknown, &challenge(Provider::Google), 1_000_000)
            .await
            .is_err());
    }
}
