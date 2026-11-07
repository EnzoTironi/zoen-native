//! Operator-only synthetic live probe; never reads a Zoen account/provider profile.
use super::*;
use serde::Deserialize;
use std::{fs, path::PathBuf};
pub(super) const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";

pub(super) fn challenge() -> Result<String, Box<dyn std::error::Error>> {
    // Do not ask the model to echo a key-shaped nonce: the unchanged outbound
    // secret detector must refuse such content, even in a synthetic probe.
    const WORDS: [&str; 16] = [
        "amber", "birch", "cedar", "dune", "elm", "fern", "glen", "hill", "iris", "jade", "kelp",
        "lake", "moss", "nest", "oak", "pine",
    ];
    let mut bytes = [0; 4];
    getrandom::getrandom(&mut bytes).map_err(|_| "synthetic challenge unavailable")?;
    let text = bytes
        .map(|byte| WORDS[usize::from(byte) % WORDS.len()])
        .join(" ");
    let checked = gate::Gate::default().check(&gate::Check::new(gate::Point::Outbound, &text));
    if checked.outcome != gate::Outcome::Allow {
        return Err("synthetic challenge is not permitted by the unchanged Gate".into());
    }
    Ok(text)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    pub endpoint: String,
    pub output_directory: PathBuf,
}
impl Profile {
    pub fn read() -> Result<Self, Box<dyn std::error::Error>> {
        if std::env::var("ZOEN_LIVE_REPLY_QA").as_deref() != Ok("1") {
            return Err("explicit live reply opt-in required".into());
        }
        let path = PathBuf::from(std::env::var("ZOEN_LIVE_REPLY_PROFILE")?);
        let profile: Self = runtime_host_config::read_private(&path)?;
        profile.validate()?;
        Ok(profile)
    }
    fn validate(&self) -> TestResult {
        let url = reqwest::Url::parse(&self.endpoint).map_err(|_| "invalid live endpoint")?;
        let account = url
            .path()
            .strip_prefix("/client/v4/accounts/")
            .and_then(|s| s.strip_suffix("/ai/v1/chat/completions"));
        if url.scheme() != "https"
            || url.host_str() != Some("api.cloudflare.com")
            || url.port().is_some_and(|p| p != 443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || account.is_none_or(|s| s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("live probe requires the exact Workers AI HTTPS route".into());
        }
        let metadata = fs::symlink_metadata(&self.output_directory)?;
        if !self.output_directory.is_absolute() || !metadata.is_dir() {
            return Err("private absolute live output directory required".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o700 {
                return Err("private live output permissions required".into());
            }
        }
        Ok(())
    }
    pub fn claim(&self) -> TestResult {
        let marker = self.output_directory.join("one-attempt.json");
        write_json(
            &marker,
            &json!({
                "synthetic_only":true,"automatic_retry":false,"model":MODEL,
                "max_input_tokens":8192,"max_output_tokens":96,"max_request_bytes":4096,
                "input_microdollars_per_million":293000,"output_microdollars_per_million":2253000,
                "pricing_reference":"https://developers.cloudflare.com/workers-ai/models/llama-3.3-70b-instruct-fp8-fast/",
                "pricing_observed":"2026-11-07","gate":"explicit_offline_floor"
            }),
        )?;
        fs::File::open(marker)?.sync_all()?;
        fs::File::open(&self.output_directory)?.sync_all()?;
        Ok(())
    }
    pub fn price(&self, value: &mut Value) {
        value["chat_endpoint_policy"] = "https".into();
        value["chat_price"] = json!({"version":"cf-llama33-20261107","endpoint":self.endpoint,
            "model":MODEL,"credential_ref":"live-chat","currency":"USD","scale":6,
            "input_units_per_million":293000,"output_units_per_million":2253000,
            "max_billable_input_tokens":8192,"max_output_tokens":96,
            "max_request_bytes":4096,"max_response_bytes":32768});
    }
}

/// Only files created beneath this probe's private operator home are retired.
/// Accounts, evidence, caller authentication and one-shot markers are preserved.
pub(super) struct CredentialCopies(pub PathBuf);
impl CredentialCopies {
    pub fn retire(&self) -> std::io::Result<()> {
        fn visit(path: &Path) -> std::io::Result<()> {
            let entries = match fs::read_dir(path) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e),
            };
            for entry in entries {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_dir() {
                    visit(&path)?;
                } else if kind.is_file()
                    && entry.file_name() == "chat"
                    && path
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|s| s == "vault")
                {
                    fs::remove_file(&path)?;
                    if let Some(parent) = path.parent() {
                        fs::File::open(parent)?.sync_all()?;
                    }
                }
            }
            Ok(())
        }
        visit(&self.0)
    }
}
impl Drop for CredentialCopies {
    fn drop(&mut self) {
        if self.retire().is_err() {
            eprintln!("live probe credential retirement failed");
        }
    }
}

#[test]
fn live_reply_profile_refuses_other_origins_and_reused_dispatch_markers() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("zoen-live-profile")?);
    fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let valid = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1/chat/completions",
        "a1".repeat(16)
    );
    let mut profile = Profile {
        endpoint: valid.clone(),
        output_directory: dir.clone(),
    };
    for endpoint in [
        valid.replace("https:", "http:"),
        valid.replace("api.cloudflare.com", "example.com"),
        valid.replace("https://", "https://user:pass@"),
        format!("{valid}?key=x"),
        format!("{valid}#x"),
        valid.replace(&"a1".repeat(16), "bad"),
    ] {
        profile.endpoint = endpoint;
        assert!(profile.validate().is_err());
    }
    profile.endpoint = valid;
    profile.validate()?;
    assert_eq!(challenge()?.split_whitespace().count(), 4);
    let mut value = json!({});
    profile.price(&mut value);
    let price: zoen_runtime::PriceProfile = serde_json::from_value(value["chat_price"].clone())?;
    let digest = price.digest()?;
    let mut other = price.clone();
    other.model = "another-provider/model:version".into();
    assert_ne!(other.digest()?, digest);
    for invalid in [
        "".into(),
        "model name".into(),
        "model\nname".into(),
        "模型".into(),
        "x".repeat(257),
    ] {
        other.model = invalid;
        assert!(other.digest().is_err());
    }
    other = price;
    other.version = "@cf/version".into();
    assert!(
        other.digest().is_err(),
        "provider syntax must not relax authority identifiers"
    );
    profile.claim()?;
    assert!(profile.claim().is_err());
    let operator = dir.join("operator");
    let vault = operator.join("vault");
    fs::create_dir_all(&vault)?;
    fs::write(vault.join("chat"), b"synthetic-token")?;
    fs::write(vault.join("evidence"), b"retained")?;
    CredentialCopies(operator).retire()?;
    assert!(!vault.join("chat").exists());
    assert!(vault.join("evidence").exists());
    assert!(dir.join("one-attempt.json").exists());
    fs::remove_dir_all(dir)?;
    Ok(())
}
