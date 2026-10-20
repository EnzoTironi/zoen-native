//! Build-provided runtime identity gates the existing catalog/resource/install path.
//! No endpoint, account, storage location or network authority is supplied by a document.
use super::{Document, APP, ENGINE_REVISION, RESOURCE};
use crate::{tool, AppSpec};
use localization::{t, ts};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use types::ActionClass;
use url::Url;

pub const HTML: &str = include_str!("../../resources/deckcraft.html");
pub const EMPTY: &[u8] = include_bytes!("../../resources/deckcraft.empty.deckcraft");
// The committed manifest is endpoint-neutral and deliberately unavailable. An explicit
// local/release build profile supplies the complete verified resource manifest.
pub const MANIFEST: &str = match option_env!("ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(manifest) => manifest,
    None => include_str!("../../resources/deckcraft.manifest.json"),
};
const MODULE_FILE: &str = "deckcraft-web-126eb4c8aba52d94.js";
const MODULE_SHA: &str = "9b0f9f19bee5f37fdc34f38ec5be9764db74f711151110c24acaf9ac14b87a06";
const WASM_FILE: &str = "deckcraft-web-5249f28be4db0259_bg.wasm";
const WASM_SHA: &str = "fb5e423ab97d8c133a67939b05fb2534f4cf452308ee9e38149e03cefc722747";
fn exact_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
    })
}
fn empty_array(value: &Value) -> bool {
    value.as_array().is_some_and(Vec::is_empty)
}
fn asset(value: &Value, file: &str, bytes: u64, hash: &str, mime: &str) -> Option<Url> {
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
        || !url.path().ends_with(&format!("/{ENGINE_REVISION}/{file}"))
    {
        return None;
    }
    Some(url)
}
/// Defensive availability check in addition to the independently verified JS loader.
/// Exact code identities change only through an explicit reviewed pinned build update.
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    if html.is_empty() || html.len() > 8 << 20 || manifest.len() > 65_536 {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(manifest) else {
        return false;
    };
    if value["id"] != APP
        || value["resourceUri"] != RESOURCE
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
        || runtime["engine"] != APP
        || runtime["revision"] != ENGINE_REVISION
        || runtime["abi"] != "zoen-host-v1"
    {
        return false;
    }
    let Some(module) = asset(
        &runtime["module"],
        MODULE_FILE,
        157_185,
        MODULE_SHA,
        "text/javascript",
    ) else {
        return false;
    };
    let Some(wasm) = asset(
        &runtime["wasm"],
        WASM_FILE,
        36_463_351,
        WASM_SHA,
        "application/wasm",
    ) else {
        return false;
    };
    module.origin() == wasm.origin()
        && module.path().rsplit_once('/').map(|p| p.0) == wasm.path().rsplit_once('/').map(|p| p.0)
}
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| valid_resource(HTML, MANIFEST) && Document::parse(EMPTY).is_ok())
}
pub fn create(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("Apresentação", "Presentation"))
        .trim();
    if !available()
        || args
            .as_object()
            .is_none_or(|o| o.keys().any(|k| k != "title"))
        || args.get("title").is_some_and(|v| !v.is_string())
        || title.is_empty()
        || title.chars().count() > 160
        || title.chars().any(char::is_control)
    {
        return Err(t(
            "Editor de apresentações indisponível ou argumentos inválidos.",
            "Presentation editor unavailable or invalid arguments.",
        ));
    }
    let doc = Document::parse(EMPTY)
        .map_err(|_| t("Documento inicial inválido.", "Invalid initial document."))?;
    Ok((
        title.into(),
        serde_json::from_str(&doc.metadata(title))
            .map_err(|_| t("Documento inicial inválido.", "Invalid initial document."))?,
    ))
}
pub fn spec() -> Option<AppSpec> {
    if !available() {
        return None;
    }
    let mut tools = vec![tool(
        "create_deckcraft",
        ts("Criar apresentação", "Create presentation"),
        ts(
            "Cria uma apresentação editável neste Espaço.",
            "Creates an editable presentation in this Space.",
        ),
        json!({"type":"object","properties":{"title":{"type":"string","maxLength":160}},"additionalProperties":false}),
        &["model"],
        ActionClass::Reversible,
        false,
    )];
    for name in [
        "app_binary_read",
        "app_binary_read_chunk",
        "app_binary_begin",
        "app_binary_append",
        "app_binary_commit",
        "app_binary_abort",
        "app_binary_export_source",
    ] {
        tools.push(tool(
            name,
            name,
            "Artifact-scoped binary document transport.",
            json!({"type":"object"}),
            &["app"],
            ActionClass::Reversible,
            matches!(
                name,
                "app_binary_read" | "app_binary_read_chunk" | "app_binary_export_source"
            ),
        ));
    }
    Some(AppSpec {
        id: APP,
        name: "DeckCraft",
        description: ts(
            "Apresentações compartilhadas e criptografadas.",
            "Shared encrypted presentations.",
        ),
        resource_uri: RESOURCE,
        html: Some(HTML),
        manifest: Some(MANIFEST),
        tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(html: &str) -> Value {
        let directory = format!("https://runtime.invalid/deckcraft/{ENGINE_REVISION}/");
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":html.len(),"sha256":format!("{:x}",Sha256::digest(html.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":ENGINE_REVISION,"abi":"zoen-host-v1","module":{"url":format!("{directory}{MODULE_FILE}"),"bytes":157185,"sha256":MODULE_SHA,"mime":"text/javascript"},"wasm":{"url":format!("{directory}{WASM_FILE}"),"bytes":36463351,"sha256":WASM_SHA,"mime":"application/wasm"}}})
    }
    #[test]
    fn runtime_config_requires_exact_html_code_identity_and_zero_guest_network_authority() {
        let html = "<!doctype html><body>synthetic configuration assertion</body>";
        let good = fixture(html);
        assert!(valid_resource(html, &good.to_string()));
        let mut absent = good.clone();
        absent.as_object_mut().unwrap().remove("runtimePackage");
        assert!(!valid_resource(html, &absent.to_string()));
        let cases=[("/runtimePackage/revision",json!("0".repeat(40))),("/runtimePackage/wasm/sha256",json!("0".repeat(64))),("/runtimePackage/module/bytes",json!(157186)),("/runtimePackage/module/url",json!(format!("https://runtime.invalid/deckcraft/{ENGINE_REVISION}/{MODULE_FILE}?data=private"))),("/runtimePackage/wasm/url",json!(format!("https://other.invalid/deckcraft/{ENGINE_REVISION}/{WASM_FILE}"))),("/runtimePackage/abi",json!("unmanaged")),("/allowedDomains",json!(["https://runtime.invalid"])),("/permissions",json!({"camera":true})),("/csp/connectDomains",json!(["https://runtime.invalid"])),("/resourceUri",json!("ui://another/artifact")),("/sha256",json!("0".repeat(64)))];
        for (pointer, value) in cases {
            let mut malformed = good.clone();
            *malformed.pointer_mut(pointer).unwrap() = value;
            assert!(!valid_resource(html, &malformed.to_string()), "{pointer}");
        }
        let mut extra = good.clone();
        extra["runtimePackage"]["network"] = json!(true);
        assert!(!valid_resource(html, &extra.to_string()));
        assert!(!valid_resource("changed html", &good.to_string()));
    }
    #[test]
    fn genuine_frozen_seed_has_one_native_slide_without_embedded_assets() {
        assert_eq!(Document::parse(EMPTY).unwrap().slides, 1);
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(EMPTY)).unwrap();
        let mut json = String::new();
        use std::io::Read;
        zip.by_name("presentation.json")
            .unwrap()
            .read_to_string(&mut json)
            .unwrap();
        assert!(!json.contains("Human edit"));
        assert!(!json.contains("Agent edit"));
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["presentation"]["media"], json!([]));
        assert_eq!(value["presentation"]["embeddedFonts"], json!([]));
    }
}
