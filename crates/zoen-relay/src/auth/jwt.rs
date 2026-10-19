use super::{Challenge, Provider};
pub(super) use relay_admission::auth::Verified;
use relay_admission::auth::{key_id, key_set, verify_with_key, Jwk, VerificationContext};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

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
        let kid = key_id(token).map_err(|_| ())?;
        let mut cache = self.caches[&challenge.provider].lock().await;
        let now_cache = Instant::now();
        let retained = cache.keys.iter().find(|key| key.kid == kid).cloned();
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
            cache.keys = key_set(&body).map_err(|_| ())?;
            cache.expires = Some(now_cache + Duration::from_secs(ttl));
        }
        let key = cache
            .keys
            .iter()
            .find(|key| key.kid == kid)
            .cloned()
            .ok_or(())?;
        drop(cache);
        verify_with_key(
            token,
            &key,
            &VerificationContext {
                provider: challenge.provider,
                audience: &challenge.audience,
                presenter: challenge.presenter.as_deref(),
                nonce: &challenge.nonce,
                created_ms: challenge.created_ms,
            },
            now,
        )
        .map_err(|_| ())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
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
        serde_json::from_str(include_str!(
            "../../../relay-admission/src/auth/fixtures/rsa-test-only.jwk.json"
        ))
        .unwrap()
    }
    fn claims() -> Value {
        json!({"iss":"https://accounts.google.com","sub":"provider-subject","aud":"audience","azp":"ios-client","iat":1000,"exp":1600,"nonce":"aa".repeat(32),"name":"Display hint"})
    }
    fn signed(header: Value, claims: Value) -> String {
        // Disposable synthetic key: never configured with either real provider.
        let key = RsaKeyPair::from_pkcs8(include_bytes!(
            "../../../relay-admission/src/auth/fixtures/rsa-test-only.pk8"
        ))
        .unwrap();
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
