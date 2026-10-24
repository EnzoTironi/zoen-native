//! Explicit build configuration gates the real, pinned WordCraft editor package.
use super::{
    resource::{self, RuntimePins},
    Format,
};
use crate::{
    tool,
    wordcraft::{Document, APP, RESOURCE},
    AppSpec,
};
use localization::{t, ts};
use serde_json::{json, Value};
use std::sync::OnceLock;
use types::ActionClass;
pub const EMPTY: &[u8] = include_bytes!("../../resources/wordcraft-empty.docx");
pub const HTML: &str = include_str!("../../resources/generated/wordcraft.html");
pub const MANIFEST: &str = match option_env!("ZOEN_WORDCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(value) => value,
    None => include_str!("../../resources/wordcraft.manifest.json"),
};
const PINS: RuntimePins = RuntimePins {
    module_file: "wordcraft.js",
    module_bytes: 80_346,
    module_sha: "89a1449b395211e425cb1d53bcacf637b35bec6f8e7edf7b75995357eb01ef59",
    wasm_file: "wordcraft_bg.wasm",
    wasm_bytes: 46_350_798,
    wasm_sha: "9931c692606084d9d54951e20343585503788ed4ac9484ab6cd1b96cebf6ae43",
};
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    resource::valid_resource(Format::Wordcraft, &PINS, html, manifest)
}
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| valid_resource(HTML, MANIFEST) && Document::parse(EMPTY).is_ok())
}
pub fn create(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("Documento", "Document"))
        .trim();
    if !available()
        || args
            .as_object()
            .is_none_or(|args| args.keys().any(|key| key != "title"))
        || args.get("title").is_some_and(|value| !value.is_string())
        || title.is_empty()
        || title.chars().count() > 160
        || title.chars().any(char::is_control)
    {
        return Err(t(
            "Editor de documentos indisponível ou argumentos inválidos.",
            "Document editor unavailable or invalid arguments.",
        ));
    }
    let doc = Document::parse(EMPTY).map_err(str::to_owned)?;
    let metadata = serde_json::from_str(&doc.metadata(title))
        .map_err(|_| "Invalid initial plain-text document metadata")?;
    Ok((title.into(), metadata))
}
pub fn spec() -> Option<AppSpec> {
    if !available() {
        return None;
    }
    let mut tools = vec![tool(
        "create_wordcraft",
        ts("Criar documento", "Create plain-text document"),
        ts(
            "Cria um documento editável nesta conversa.",
            "Creates an editable plain-text document in this conversation.",
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
        name: "WordCraft",
        description: ts(
            "Documentos compartilhados e criptografados.",
            "Shared encrypted plain-text documents.",
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
    use sha2::{Digest, Sha256};
    fn fixture() -> Value {
        let base = format!(
            "https://runtime.invalid/wordcraft/{}/",
            Format::Wordcraft.revision()
        );
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":HTML.len(),"sha256":format!("{:x}",Sha256::digest(HTML.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":Format::Wordcraft.revision(),"abi":"zoen-host-v1","module":{"url":format!("{base}{}",PINS.module_file),"bytes":PINS.module_bytes,"sha256":PINS.module_sha,"mime":"text/javascript"},"wasm":{"url":format!("{base}{}",PINS.wasm_file),"bytes":PINS.wasm_bytes,"sha256":PINS.wasm_sha,"mime":"application/wasm"}}})
    }
    #[test]
    fn word_factory_requires_exact_real_package_and_never_default_enables() {
        assert_eq!(Document::parse(EMPTY).expect("real writer seed").text, "");
        assert!(valid_resource(HTML, &fixture().to_string()));
        assert_eq!(
            available(),
            option_env!("ZOEN_WORDCRAFT_RESOURCE_MANIFEST_JSON").is_some()
        );
        assert_eq!(spec().is_some(), available());
        assert_eq!(
            create(&json!({"title":"Shared document"})).is_ok(),
            available()
        );
        for args in [
            json!({"title":null}),
            json!({"title":""}),
            json!({"title":"doc","actor":"forged"}),
        ] {
            assert!(create(&args).is_err());
        }
    }
    #[test]
    fn word_registration_refuses_wrong_engine_asset_hash_abi_and_permissions() {
        let good = fixture();
        for (path, value) in [
            ("/runtimePackage/engine", json!("gridcraft")),
            ("/runtimePackage/revision", json!("foreign")),
            ("/runtimePackage/abi", json!("other")),
            ("/runtimePackage/module/sha256", json!("0".repeat(64))),
            ("/runtimePackage/wasm/bytes", json!(1)),
            ("/permissions", json!({"camera":{}})),
            ("/csp/connectDomains", json!(["https://example.invalid"])),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(path).expect("fixture field") = value;
            assert!(!valid_resource(HTML, &bad.to_string()), "{path}");
        }
    }
}
