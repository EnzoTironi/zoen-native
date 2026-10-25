//! Explicit build configuration gates the real, pinned PhotoCraft editor package.
use super::{
    resource::{self, RuntimePins},
    Format,
};
use crate::{
    photocraft::{Document, APP, RESOURCE},
    tool, AppSpec,
};
use localization::{t, ts};
use serde_json::{json, Value};
use std::sync::OnceLock;
use types::ActionClass;
pub const EMPTY: &[u8] = include_bytes!("../../resources/photocraft-empty.pcraft");
pub const HTML: &str = include_str!("../../resources/generated/photocraft.html");
pub const MANIFEST: &str = match option_env!("ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(value) => value,
    None => include_str!("../../resources/photocraft.manifest.json"),
};
const PINS: RuntimePins = RuntimePins {
    module_file: "photocraft.js",
    module_bytes: 158764,
    module_sha: "b2e2968f4dc9b55be26c9da07ea5c23d5a03612d658ede1058d4c38388455ac8",
    wasm_file: "photocraft_bg.wasm",
    wasm_bytes: 32678991,
    wasm_sha: "25a4ebfcef19d0527fa8ea340a89b81b6da24908eb781c3d62318e2c216a8592",
};
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    resource::valid_resource(Format::Photocraft, &PINS, html, manifest)
}
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| valid_resource(HTML, MANIFEST) && Document::parse(EMPTY).is_ok())
}
pub fn create(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("Imagem", "Image"))
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
            "Editor de imagens indisponível ou argumentos inválidos.",
            "Image editor unavailable or invalid arguments.",
        ));
    }
    let doc = Document::parse(EMPTY).map_err(str::to_owned)?;
    let metadata = serde_json::from_str(&doc.metadata(title))
        .map_err(|_| "Invalid initial raster metadata")?;
    Ok((title.into(), metadata))
}
pub fn spec() -> Option<AppSpec> {
    if !available() {
        return None;
    }
    let mut tools = vec![tool(
        "create_photocraft",
        ts("Criar imagem", "Create image"),
        ts(
            "Cria uma imagem editável nesta conversa.",
            "Creates an editable image in this conversation.",
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
        name: "PhotoCraft",
        description: ts(
            "Imagens compartilhadas e criptografadas.",
            "Shared encrypted images.",
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
            "https://runtime.invalid/photocraft/{}/",
            Format::Photocraft.revision()
        );
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":HTML.len(),"sha256":format!("{:x}",Sha256::digest(HTML.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":Format::Photocraft.revision(),"abi":"zoen-host-v1","module":{"url":format!("{base}{}",PINS.module_file),"bytes":PINS.module_bytes,"sha256":PINS.module_sha,"mime":"text/javascript"},"wasm":{"url":format!("{base}{}",PINS.wasm_file),"bytes":PINS.wasm_bytes,"sha256":PINS.wasm_sha,"mime":"application/wasm"}}})
    }
    #[test]
    fn photo_factory_requires_exact_real_package_and_never_default_enables() {
        assert_eq!(Document::parse(EMPTY).expect("real writer seed").width, 640);
        assert!(valid_resource(HTML, &fixture().to_string()));
        assert_eq!(
            available(),
            option_env!("ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON").is_some()
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
    fn photo_registration_refuses_wrong_engine_asset_hash_abi_and_permissions() {
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
