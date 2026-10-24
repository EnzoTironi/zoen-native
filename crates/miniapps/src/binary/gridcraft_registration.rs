//! Explicit build configuration gates the real, pinned GridCraft editor package.
use super::{
    resource::{self, RuntimePins},
    Format,
};
use crate::{
    gridcraft::{Document, APP, RESOURCE},
    tool, AppSpec,
};
use localization::{t, ts};
use serde_json::{json, Value};
use std::sync::OnceLock;
use types::ActionClass;
pub const EMPTY: &[u8] = include_bytes!("../../resources/gridcraft-empty.xlsx");
pub const HTML: &str = include_str!("../../resources/generated/gridcraft.html");
pub const MANIFEST: &str = match option_env!("ZOEN_GRIDCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(value) => value,
    None => include_str!("../../resources/gridcraft.manifest.json"),
};
const PINS: RuntimePins = RuntimePins {
    module_file: "gridcraft-web-dab23e6ef6507ebe.js",
    module_bytes: 77_935,
    module_sha: "dab23e6ef6507ebe066f66b0c83a8ae2b94f03fc988f39c1ac2776ce1821f511",
    wasm_file: "gridcraft-web-5153bf63fb5e9877_bg.wasm",
    wasm_bytes: 20_192_554,
    wasm_sha: "5153bf63fb5e9877655446b23e4ade78d7207dbcd7f99d71bdfbf549d161384c",
};
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    resource::valid_resource(Format::Gridcraft, &PINS, html, manifest)
}
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| valid_resource(HTML, MANIFEST) && Document::parse(EMPTY).is_ok())
}
pub fn create(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("Planilha", "Spreadsheet"))
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
            "Editor de planilhas indisponível ou argumentos inválidos.",
            "Spreadsheet editor unavailable or invalid arguments.",
        ));
    }
    let doc = Document::parse(EMPTY).map_err(str::to_owned)?;
    let metadata = serde_json::from_str(&doc.metadata(title))
        .map_err(|_| "Invalid initial spreadsheet metadata")?;
    Ok((title.into(), metadata))
}
pub fn spec() -> Option<AppSpec> {
    if !available() {
        return None;
    }
    let mut tools = vec![tool(
        "create_gridcraft",
        ts("Criar planilha", "Create spreadsheet"),
        ts(
            "Cria uma planilha editável nesta conversa.",
            "Creates an editable spreadsheet in this conversation.",
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
        name: "GridCraft",
        description: ts(
            "Planilhas compartilhadas e criptografadas.",
            "Shared encrypted spreadsheets.",
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
            "https://runtime.invalid/gridcraft/{}/",
            Format::Gridcraft.revision()
        );
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":HTML.len(),"sha256":format!("{:x}",Sha256::digest(HTML.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":Format::Gridcraft.revision(),"abi":"zoen-host-v1","module":{"url":format!("{base}{}",PINS.module_file),"bytes":PINS.module_bytes,"sha256":PINS.module_sha,"mime":"text/javascript"},"wasm":{"url":format!("{base}{}",PINS.wasm_file),"bytes":PINS.wasm_bytes,"sha256":PINS.wasm_sha,"mime":"application/wasm"}}})
    }
    #[test]
    fn real_seed_and_exact_package_are_required_before_discovery() {
        assert_eq!(Document::parse(EMPTY).expect("actual engine seed").cells, 0);
        assert!(valid_resource(HTML, &fixture().to_string()));
        if option_env!("ZOEN_GRIDCRAFT_RESOURCE_MANIFEST_JSON").is_some() {
            assert!(available());
            assert!(spec().is_some());
        } else {
            assert!(!available());
            assert!(spec().is_none());
            assert!(create(&json!({})).is_err());
        }
    }
    #[test]
    fn forged_package_identity_or_guest_network_permissions_cannot_enable_grid() {
        let good = fixture();
        for (path, value) in [
            ("/runtimePackage/engine", json!("deckcraft")),
            ("/runtimePackage/module/sha256", json!("0".repeat(64))),
            ("/runtimePackage/wasm/bytes", json!(1)),
            (
                "/runtimePackage/wasm/url",
                json!("https://other.invalid/missing.wasm"),
            ),
            (
                "/capabilities",
                json!([{"id":"network","purpose":"forged"}]),
            ),
            ("/csp/connectDomains", json!(["https://example.invalid"])),
            ("/permissions", json!({"camera":{}})),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(path).expect("fixture path") = value;
            assert!(!valid_resource(HTML, &bad.to_string()), "{path}");
        }
    }
}
