//! The existing zero-egress runtime/resource checks shared by two fixed formats.
use super::Format;
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;
pub(crate) struct RuntimePins {
    pub module_file: &'static str,
    pub module_bytes: u64,
    pub module_sha: &'static str,
    pub wasm_file: &'static str,
    pub wasm_bytes: u64,
    pub wasm_sha: &'static str,
}
fn exact_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
    })
}
fn empty_array(value: &Value) -> bool {
    value.as_array().is_some_and(Vec::is_empty)
}
fn asset(
    value: &Value,
    file: &str,
    bytes: u64,
    hash: &str,
    mime: &str,
    revision: &str,
) -> Option<Url> {
    if !exact_keys(value, &["url", "bytes", "sha256", "mime"])
        || value["bytes"].as_u64() != Some(bytes)
        || value["sha256"].as_str() != Some(hash)
        || value["mime"].as_str() != Some(mime)
    {
        return None;
    }
    let source = value["url"].as_str()?;
    let url = Url::parse(source).ok()?;
    if source.len() > 2048
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != source
        || !url
            .path()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_.-,".contains(&b))
        || url.path().contains("//")
        || !url.path().ends_with(&format!("/{revision}/{file}"))
    {
        return None;
    }
    Some(url)
}
/// Defensive availability check in addition to the independently verified JS loader.
/// Exact code identities change only through an explicit reviewed pinned build update.
pub(crate) fn valid_resource(
    format: Format,
    pins: &RuntimePins,
    html: &str,
    manifest: &str,
) -> bool {
    if html.is_empty() || html.len() > 8 << 20 || manifest.len() > 65_536 {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(manifest) else {
        return false;
    };
    if value["id"] != format.id()
        || value["resourceUri"] != format.resource()
        || value["mimeType"] != crate::MIME
        || value["bytes"].as_u64() != Some(html.len() as u64)
        || value["sha256"].as_str()
            != Some(format!("{:x}", Sha256::digest(html.as_bytes())).as_str())
        || !empty_array(&value["capabilities"])
        || !empty_array(&value["allowedDomains"])
        || !empty_array(&value["linkDomains"])
        || !value["permissions"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        || !exact_keys(
            &value["csp"],
            &[
                "connectDomains",
                "resourceDomains",
                "frameDomains",
                "baseUriDomains",
            ],
        )
        || ![
            "connectDomains",
            "resourceDomains",
            "frameDomains",
            "baseUriDomains",
        ]
        .iter()
        .all(|key| empty_array(&value["csp"][*key]))
    {
        return false;
    }
    let runtime = &value["runtimePackage"];
    if !exact_keys(
        runtime,
        &["schema", "engine", "revision", "abi", "module", "wasm"],
    ) || runtime["schema"].as_u64() != Some(1)
        || runtime["engine"] != format.id()
        || runtime["revision"] != format.revision()
        || runtime["abi"] != "zoen-host-v1"
    {
        return false;
    }
    let Some(module) = asset(
        &runtime["module"],
        pins.module_file,
        pins.module_bytes,
        pins.module_sha,
        "text/javascript",
        format.revision(),
    ) else {
        return false;
    };
    let Some(wasm) = asset(
        &runtime["wasm"],
        pins.wasm_file,
        pins.wasm_bytes,
        pins.wasm_sha,
        "application/wasm",
        format.revision(),
    ) else {
        return false;
    };
    module.origin() == wasm.origin()
        && module.path().rsplit_once('/').map(|p| p.0) == wasm.path().rsplit_once('/').map(|p| p.0)
}
