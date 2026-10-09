//! Web Bot Auth: Zoen's agents sign their HTTP requests so sites (Cloudflare's verified bots
//! and signed agents, and anyone else implementing the draft) can tell a Zoen agent from a
//! scraper pretending to be one, instead of sending it a CAPTCHA (ADR 0028 §7).
//!
//! - **What is signed** (RFC 9421 HTTP Message Signatures, Ed25519): the request's
//!   `@authority` and the `Signature-Agent` header, which names where the key directory
//!   lives. Parameters: `created`, `expires` (5 minutes later), `keyid` (the JWK thumbprint of
//!   the key, RFC 7638), `alg="ed25519"`, a random `nonce`, and `tag="web-bot-auth"`.
//! - **Where.** The egress proxy adds the headers to requests it can see: plain HTTP, and
//!   HTTPS it intercepts (a browser lease's CA covers its allowlisted hosts). The sandbox never
//!   holds the key, and any `Signature*` headers it sends are dropped first.
//! - **The key directory** is served at `/.well-known/http-message-signatures-directory` on
//!   the `Signature-Agent` origin: a JWKS (`kty: OKP`, `crv: Ed25519`), content type
//!   `application/http-message-signatures-directory+json`, itself signed over
//!   `"@authority";req` with `tag="http-message-signatures-directory"`.
//!
//! [`verify_request`] checks a request the way a site does; journeys use it as the origin.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64URL};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const DIRECTORY_PATH: &str = "/.well-known/http-message-signatures-directory";
pub const DIRECTORY_CONTENT_TYPE: &str = "application/http-message-signatures-directory+json";
pub const TAG_REQUEST: &str = "web-bot-auth";
pub const TAG_DIRECTORY: &str = "http-message-signatures-directory";
/// How long a request signature is valid.
pub const VALIDITY_SECS: u64 = 300;

/// The operator's signing identity: one Ed25519 key and the origin that publishes it.
pub struct SignedAgent {
    key: SigningKey,
    /// e.g. `https://agents.zoen.app` (no trailing slash).
    agent_url: String,
}

impl std::fmt::Debug for SignedAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedAgent")
            .field("agent_url", &self.agent_url)
            .field("keyid", &self.keyid())
            .finish_non_exhaustive()
    }
}

/// RFC 7638 thumbprint of an Ed25519 public key as an OKP JWK.
pub fn jwk_thumbprint(public: &[u8; 32]) -> String {
    // Members in lexicographic order, no whitespace.
    let canonical = format!(
        r#"{{"crv":"Ed25519","kty":"OKP","x":"{}"}}"#,
        B64URL.encode(public)
    );
    B64URL.encode(Sha256::digest(canonical.as_bytes()))
}

fn nonce() -> String {
    let mut n = [0u8; 32];
    getrandom::getrandom(&mut n).expect("os randomness");
    B64.encode(n)
}

/// `"name": value` lines plus the `@signature-params` line, joined by `\n`.
pub fn signature_base(components: &[(String, String)], params: &str) -> String {
    let mut lines: Vec<String> = components
        .iter()
        .map(|(n, v)| format!("{n}: {v}"))
        .collect();
    lines.push(format!("\"@signature-params\": {params}"));
    lines.join("\n")
}

impl SignedAgent {
    pub fn from_seed(seed: [u8; 32], agent_url: &str) -> Self {
        SignedAgent {
            key: SigningKey::from_bytes(&seed),
            agent_url: agent_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn generate(agent_url: &str) -> Self {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).expect("os randomness");
        Self::from_seed(seed, agent_url)
    }

    pub fn agent_url(&self) -> &str {
        &self.agent_url
    }

    pub fn public(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    pub fn keyid(&self) -> String {
        jwk_thumbprint(&self.public())
    }

    /// The JWKS the directory serves.
    pub fn jwks(&self) -> serde_json::Value {
        json!({ "keys": [ { "kty": "OKP", "crv": "Ed25519", "x": B64URL.encode(self.public()) } ] })
    }

    /// The `Signature-Agent`, `Signature-Input` and `Signature` headers for a request to
    /// `authority` (host, plus `:port` when not the scheme's default).
    pub fn sign_request(&self, authority: &str, now_s: u64) -> Vec<(String, String)> {
        let agent = format!("\"{}\"", self.agent_url);
        let params = format!(
            "(\"@authority\" \"signature-agent\");created={now_s};keyid=\"{}\";alg=\"ed25519\";expires={};nonce=\"{}\";tag=\"{TAG_REQUEST}\"",
            self.keyid(),
            now_s + VALIDITY_SECS,
            nonce(),
        );
        let base = signature_base(
            &[
                ("\"@authority\"".into(), authority.to_ascii_lowercase()),
                ("\"signature-agent\"".into(), agent.clone()),
            ],
            &params,
        );
        let sig = self.key.sign(base.as_bytes());
        vec![
            ("Signature-Agent".into(), agent),
            ("Signature-Input".into(), format!("sig1={params}")),
            (
                "Signature".into(),
                format!("sig1=:{}:", B64.encode(sig.to_bytes())),
            ),
        ]
    }

    /// The directory response to a request for `authority` (the agent origin's host):
    /// headers and body.
    pub fn directory_response(
        &self,
        authority: &str,
        now_s: u64,
    ) -> (Vec<(String, String)>, String) {
        let params = format!(
            "(\"@authority\";req);created={now_s};keyid=\"{}\";alg=\"ed25519\";expires={};tag=\"{TAG_DIRECTORY}\"",
            self.keyid(),
            now_s + VALIDITY_SECS,
        );
        let base = signature_base(
            &[("\"@authority\";req".into(), authority.to_ascii_lowercase())],
            &params,
        );
        let sig = self.key.sign(base.as_bytes());
        (
            vec![
                ("Content-Type".into(), DIRECTORY_CONTENT_TYPE.into()),
                ("Signature-Input".into(), format!("sig1={params}")),
                (
                    "Signature".into(),
                    format!("sig1=:{}:", B64.encode(sig.to_bytes())),
                ),
                ("Cache-Control".into(), "max-age=86400".into()),
            ],
            self.jwks().to_string(),
        )
    }
}

/// What a verifier learned from a valid signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub keyid: String,
    pub agent: String,
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn param<'a>(params: &'a str, name: &str) -> Option<&'a str> {
    params.split(';').skip(1).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| v.trim_matches('"'))
    })
}

/// Checks a signed request the way a site would: the covered components, the tag, the time
/// window, and the Ed25519 signature against a key in `jwks` with the matching thumbprint.
pub fn verify_request(
    headers: &[(String, String)],
    authority: &str,
    jwks: &serde_json::Value,
    now_s: u64,
) -> Result<Verified, String> {
    let input = header(headers, "signature-input").ok_or("no Signature-Input")?;
    let sig = header(headers, "signature").ok_or("no Signature")?;
    let agent = header(headers, "signature-agent").ok_or("no Signature-Agent")?;
    let params = input.strip_prefix("sig1=").ok_or("no sig1 label")?;
    if !params.starts_with("(\"@authority\" \"signature-agent\")") {
        return Err("unexpected covered components".into());
    }
    if param(params, "tag") != Some(TAG_REQUEST) {
        return Err("wrong tag".into());
    }
    if param(params, "alg") != Some("ed25519") {
        return Err("wrong alg".into());
    }
    let created: u64 = param(params, "created")
        .and_then(|v| v.parse().ok())
        .ok_or("no created")?;
    let expires: u64 = param(params, "expires")
        .and_then(|v| v.parse().ok())
        .ok_or("no expires")?;
    if now_s + 5 < created || now_s > expires {
        return Err("outside the validity window".into());
    }
    let keyid = param(params, "keyid").ok_or("no keyid")?;
    let key = jwks["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| k["kty"] == "OKP" && k["crv"] == "Ed25519")
        .filter_map(|k| B64URL.decode(k["x"].as_str()?).ok())
        .filter_map(|x| <[u8; 32]>::try_from(x).ok())
        .find(|x| jwk_thumbprint(x) == keyid)
        .ok_or("unknown keyid")?;
    let sig_b64 = sig
        .strip_prefix("sig1=:")
        .and_then(|s| s.strip_suffix(':'))
        .ok_or("bad Signature")?;
    let sig_bytes: [u8; 64] = B64
        .decode(sig_b64)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("bad signature bytes")?;
    let base = signature_base(
        &[
            ("\"@authority\"".into(), authority.to_ascii_lowercase()),
            ("\"signature-agent\"".into(), agent.to_string()),
        ],
        params,
    );
    VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify(base.as_bytes(), &Signature::from_bytes(&sig_bytes))
        .map_err(|_| "signature does not verify".to_string())?;
    Ok(Verified {
        keyid: keyid.to_string(),
        agent: agent.trim_matches('"').to_string(),
    })
}

/// Checks a directory response's own signature (the `"@authority";req` form).
pub fn verify_directory(
    headers: &[(String, String)],
    body: &str,
    authority: &str,
    now_s: u64,
) -> Result<serde_json::Value, String> {
    if header(headers, "content-type") != Some(DIRECTORY_CONTENT_TYPE) {
        return Err("wrong content type".into());
    }
    let jwks: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let input = header(headers, "signature-input").ok_or("no Signature-Input")?;
    let sig = header(headers, "signature").ok_or("no Signature")?;
    let params = input.strip_prefix("sig1=").ok_or("no sig1 label")?;
    if param(params, "tag") != Some(TAG_DIRECTORY) {
        return Err("wrong tag".into());
    }
    let expires: u64 = param(params, "expires")
        .and_then(|v| v.parse().ok())
        .ok_or("no expires")?;
    if now_s > expires {
        return Err("expired".into());
    }
    let keyid = param(params, "keyid").ok_or("no keyid")?;
    let key = jwks["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|k| B64URL.decode(k["x"].as_str()?).ok())
        .filter_map(|x| <[u8; 32]>::try_from(x).ok())
        .find(|x| jwk_thumbprint(x) == keyid)
        .ok_or("directory not signed by one of its keys")?;
    let sig_bytes: [u8; 64] = sig
        .strip_prefix("sig1=:")
        .and_then(|s| s.strip_suffix(':'))
        .and_then(|s| B64.decode(s).ok())
        .and_then(|b| b.try_into().ok())
        .ok_or("bad Signature")?;
    let base = signature_base(
        &[("\"@authority\";req".into(), authority.to_ascii_lowercase())],
        params,
    );
    VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify(base.as_bytes(), &Signature::from_bytes(&sig_bytes))
        .map_err(|_| "directory signature does not verify".to_string())?;
    Ok(jwks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7638_thumbprint_matches_the_published_example() {
        // The key from the Web Bot Auth architecture draft (Appendix): Ed25519 public key
        // JrQLj5P_89iXES9-vFgrIy29clF9CC_oPPsw3c5D0bs, thumbprint poqkLGiymh_W0uP6PZFw-dvez3QJT5SolqXBCW38r0U.
        let x: [u8; 32] = B64URL
            .decode("JrQLj5P_89iXES9-vFgrIy29clF9CC_oPPsw3c5D0bs")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(
            jwk_thumbprint(&x),
            "poqkLGiymh_W0uP6PZFw-dvez3QJT5SolqXBCW38r0U"
        );
    }

    #[test]
    fn a_signed_request_verifies_and_a_changed_one_does_not() {
        let a = SignedAgent::generate("https://agents.zoen.test/");
        let now = 1_800_000_000;
        let h = a.sign_request("shop.example.com", now);
        assert_eq!(h[0].1, "\"https://agents.zoen.test\"");
        assert!(h[1].1.contains("tag=\"web-bot-auth\""));
        let v = verify_request(&h, "shop.example.com", &a.jwks(), now + 10).unwrap();
        assert_eq!(v.keyid, a.keyid());
        assert_eq!(v.agent, "https://agents.zoen.test");
        // Another site, too late, another key.
        assert!(verify_request(&h, "evil.example.com", &a.jwks(), now).is_err());
        assert!(verify_request(&h, "shop.example.com", &a.jwks(), now + 301).is_err());
        let b = SignedAgent::generate("https://agents.zoen.test");
        assert!(verify_request(&h, "shop.example.com", &b.jwks(), now).is_err());
        // A swapped Signature-Agent.
        let mut h2 = h.clone();
        h2[0].1 = "\"https://evil.test\"".into();
        assert!(verify_request(&h2, "shop.example.com", &a.jwks(), now).is_err());
    }

    #[test]
    fn the_directory_is_signed_by_its_own_key() {
        let a = SignedAgent::generate("https://agents.zoen.test");
        let (h, body) = a.directory_response("agents.zoen.test", 1_800_000_000);
        let jwks = verify_directory(&h, &body, "agents.zoen.test", 1_800_000_001).unwrap();
        assert_eq!(jwks["keys"][0]["crv"], "Ed25519");
        assert!(verify_directory(&h, &body, "other.test", 1_800_000_001).is_err());
    }
}
