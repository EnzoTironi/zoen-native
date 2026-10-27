//! Explicit build configuration gates the real, pinned `PdfCraft` editor package.
#![warn(clippy::pedantic, clippy::perf)]
use super::{
    resource::{self, RuntimePins},
    Format,
};
use crate::{
    pdfcraft::{Document, APP, RESOURCE},
    tool, AppSpec,
};
use localization::{t, ts};
use serde_json::{json, Value};
use std::sync::OnceLock;
use types::ActionClass;
pub const EMPTY: &[u8] = include_bytes!("../../resources/pdfcraft-empty.pdf");
pub const HTML: &str = include_str!("../../resources/generated/pdfcraft.html");
pub const MANIFEST: &str = match option_env!("ZOEN_PDFCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(value) => value,
    None => include_str!("../../resources/pdfcraft.manifest.json"),
};
const PINS: RuntimePins = RuntimePins {
    module_file: "pdfcraft.js",
    module_bytes: 164_127,
    module_sha: "192e0929e2866efb678e5444978b3aab1dc058da7ccabd9813b7faea75d9a81c",
    wasm_file: "pdfcraft_bg.wasm",
    wasm_bytes: 48_700_397,
    wasm_sha: "6ba1d1dccbb15674aa1e4b03d42b6c16a24d8d54dfe71cad640482eade8e9eac",
};
#[must_use]
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    resource::valid_resource(Format::Pdfcraft, &PINS, html, manifest)
}
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| valid_resource(HTML, MANIFEST) && Document::parse(EMPTY).is_ok())
}
/// # Errors
/// Refuses an unavailable runtime, invalid title or unexpected argument.
pub fn create(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("PDF", "PDF"))
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
            "Editor de PDF indisponível ou argumentos inválidos.",
            "PDF editor unavailable or invalid arguments.",
        ));
    }
    let doc = Document::parse(EMPTY).map_err(str::to_owned)?;
    let metadata =
        serde_json::from_str(&doc.metadata(title)).map_err(|_| "Invalid initial PDF metadata")?;
    Ok((title.into(), metadata))
}
#[must_use]
pub fn spec() -> Option<AppSpec> {
    if !available() {
        return None;
    }
    let mut tools = vec![tool(
        "create_pdfcraft",
        ts("Criar PDF", "Create PDF"),
        ts(
            "Cria um PDF editável nesta conversa.",
            "Creates an editable PDF in this conversation.",
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
        name: "PdfCraft",
        description: ts(
            "PDFs compartilhados e criptografados.",
            "Shared encrypted PDFs.",
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
            "https://runtime.invalid/pdfcraft/{}/",
            Format::Pdfcraft.revision()
        );
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":HTML.len(),"sha256":format!("{:x}",Sha256::digest(HTML.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":Format::Pdfcraft.revision(),"abi":"zoen-host-v1","module":{"url":format!("{base}{}",PINS.module_file),"bytes":PINS.module_bytes,"sha256":PINS.module_sha,"mime":"text/javascript"},"wasm":{"url":format!("{base}{}",PINS.wasm_file),"bytes":PINS.wasm_bytes,"sha256":PINS.wasm_sha,"mime":"application/wasm"}}})
    }
    #[test]
    fn pdf_factory_requires_exact_real_package_and_never_default_enables() {
        assert_eq!(
            Document::parse(EMPTY)
                .expect("real writer seed")
                .declared_pages,
            1
        );
        assert!(valid_resource(HTML, &fixture().to_string()));
        assert_eq!(
            available(),
            option_env!("ZOEN_PDFCRAFT_RESOURCE_MANIFEST_JSON").is_some()
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
    fn pdf_registration_refuses_wrong_engine_asset_hash_abi_and_permissions() {
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
