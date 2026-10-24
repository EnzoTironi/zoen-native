//! Build-provided runtime identity gates the existing catalog/resource/install path.
//! No endpoint, account, storage location or network authority is supplied by a document.
use super::{Document, APP, ENGINE_REVISION, RESOURCE};
use crate::{tool, AppSpec};
use localization::{t, ts};
use serde_json::{json, Value};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use types::ActionClass;

pub const HTML: &str = include_str!("../../resources/deckcraft.html");
pub const EMPTY: &[u8] = include_bytes!("../../resources/deckcraft.empty.deckcraft");
// The committed manifest is endpoint-neutral and deliberately unavailable. An explicit
// local/release build profile supplies the complete verified resource manifest.
pub const MANIFEST: &str = match option_env!("ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON") {
    Some(manifest) => manifest,
    None => include_str!("../../resources/deckcraft.manifest.json"),
};
// Candidate 06 admits a genuine Notes click before the DOM viewport gains focus.
// Native keyboard, edit/save and export acceptance remain separate.
const MODULE_FILE: &str = "deckcraft-web-4576e715d9c77da4.js";
const MODULE_BYTES: u64 = 157_851;
const MODULE_SHA: &str = "7144d8766ed42db97e9f8366515328c7ef353a04d80d66c213ae107619c12ead";
const WASM_FILE: &str = "deckcraft-web-b023d45fa5023a1a_bg.wasm";
const WASM_BYTES: u64 = 36_382_474;
const WASM_SHA: &str = "c9ff98889b779e3c11661e290c1c74317fd6aa9eecbe7705c085f7ff74cd4c0e";
pub fn valid_resource(html: &str, manifest: &str) -> bool {
    crate::binary::resource::valid_resource(
        crate::binary::Format::Deckcraft,
        &crate::binary::resource::RuntimePins {
            module_file: MODULE_FILE,
            module_bytes: MODULE_BYTES,
            module_sha: MODULE_SHA,
            wasm_file: WASM_FILE,
            wasm_bytes: WASM_BYTES,
            wasm_sha: WASM_SHA,
        },
        html,
        manifest,
    )
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

    #[test]
    fn configured_runtime_opens_the_catalog_and_genuine_seed_without_document_supplied_code() {
        if option_env!("ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON").is_some() {
            assert!(valid_resource(HTML, MANIFEST));
            let registered = spec().expect("the explicitly configured package must be available");
            assert_eq!(registered.resource_uri, RESOURCE);
            assert_eq!(registered.manifest, Some(MANIFEST));
            let (title, metadata) = create(&json!({"title":"Configured presentation"})).unwrap();
            assert_eq!(title, "Configured presentation");
            assert_eq!(metadata["slides"], 1);
        } else {
            assert!(
                !available(),
                "the endpoint-neutral default cannot expose a runtime"
            );
            assert!(spec().is_none());
            assert!(create(&json!({"title":"Unavailable presentation"})).is_err());
        }
    }

    fn fixture(html: &str) -> Value {
        let directory = format!("https://runtime.invalid/deckcraft/{ENGINE_REVISION}/");
        json!({"id":APP,"resourceUri":RESOURCE,"mimeType":crate::MIME,"bytes":html.len(),"sha256":format!("{:x}",Sha256::digest(html.as_bytes())),"capabilities":[],"allowedDomains":[],"linkDomains":[],"permissions":{},"csp":{"connectDomains":[],"resourceDomains":[],"frameDomains":[],"baseUriDomains":[]},"runtimePackage":{"schema":1,"engine":APP,"revision":ENGINE_REVISION,"abi":"zoen-host-v1","module":{"url":format!("{directory}{MODULE_FILE}"),"bytes":MODULE_BYTES,"sha256":MODULE_SHA,"mime":"text/javascript"},"wasm":{"url":format!("{directory}{WASM_FILE}"),"bytes":WASM_BYTES,"sha256":WASM_SHA,"mime":"application/wasm"}}})
    }
    #[test]
    fn runtime_config_requires_exact_html_code_identity_and_zero_guest_network_authority() {
        let html = "<!doctype html><body>synthetic configuration assertion</body>";
        let good = fixture(html);
        assert!(valid_resource(html, &good.to_string()));
        let mut absent = good.clone();
        absent.as_object_mut().unwrap().remove("runtimePackage");
        assert!(!valid_resource(html, &absent.to_string()));
        let cases = [
            ("/runtimePackage/revision", json!("0".repeat(40))),
            ("/runtimePackage/wasm/sha256", json!("0".repeat(64))),
            ("/runtimePackage/module/bytes", json!(157852)),
            (
                "/runtimePackage/module/url",
                json!(format!(
                    "https://runtime.invalid/deckcraft/{ENGINE_REVISION}/{MODULE_FILE}?data=private"
                )),
            ),
            (
                "/runtimePackage/wasm/url",
                json!(format!(
                    "https://other.invalid/deckcraft/{ENGINE_REVISION}/{WASM_FILE}"
                )),
            ),
            ("/runtimePackage/abi", json!("unmanaged")),
            ("/allowedDomains", json!(["https://runtime.invalid"])),
            ("/permissions", json!({"camera":true})),
            ("/csp/connectDomains", json!(["https://runtime.invalid"])),
            ("/resourceUri", json!("ui://another/artifact")),
            ("/sha256", json!("0".repeat(64))),
        ];
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
    fn pinned_input_runtime_upgrade_rejects_the_previous_pair_and_mixed_code_identities() {
        let html = "<!doctype html><body>pinned input runtime upgrade assertion</body>";
        let good = fixture(html);
        assert!(valid_resource(html, &good.to_string()));
        let directory = format!("https://runtime.invalid/deckcraft/{ENGINE_REVISION}/");
        // Older contact/input packages never enter through the current identity.
        let mut previous_contact = good.clone();
        previous_contact["runtimePackage"]["wasm"] = json!({
            "url":format!("{directory}deckcraft-web-f96d04a08b130930_bg.wasm"),
            "bytes":36395766,
            "sha256":"88677fbf8df8dfe3f4ae3db408eda558b7c02179626750e6fb8b12fec33e767d",
            "mime":"application/wasm"
        });
        assert!(!valid_resource(html, &previous_contact.to_string()));
        let old_module = json!({
            "url":format!("{directory}deckcraft-web-d31e46606624b37e.js"),
            "bytes":157185,
            "sha256":"ee3453072b20bacdee76f3a51b54a75963d0d2a68e70988b3e7bb6b6152deb73",
            "mime":"text/javascript"
        });
        let old_wasm = json!({
            "url":format!("{directory}deckcraft-web-12a58bf39c518c12_bg.wasm"),
            "bytes":36294009,
            "sha256":"1d93416d46bf0c3aa2802e6f0eaf3e6b1c6c31ae0c105082e9d3ca0de14f7bf3",
            "mime":"application/wasm"
        });
        for (module, wasm) in [
            (old_module.clone(), old_wasm.clone()),
            (old_module, good["runtimePackage"]["wasm"].clone()),
            (good["runtimePackage"]["module"].clone(), old_wasm),
        ] {
            let mut retired = good.clone();
            retired["runtimePackage"]["module"] = module;
            retired["runtimePackage"]["wasm"] = wasm;
            assert!(!valid_resource(html, &retired.to_string()));
        }
        // Each descriptor independently pins name, byte count and digest, not only its pair.
        for (pointer, value) in [
            ("/runtimePackage/module/sha256", json!("0".repeat(64))),
            ("/runtimePackage/wasm/bytes", json!(36387051)),
            (
                "/runtimePackage/module/url",
                json!(format!("{directory}deckcraft-web-d31e46606624b37e.js")),
            ),
        ] {
            let mut changed = good.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(!valid_resource(html, &changed.to_string()), "{pointer}");
        }
    }
    #[test]
    fn unfocused_notes_candidate_requires_its_exact_pair_and_rejects_retired_or_mixed_assets() {
        let directory = format!("https://runtime.invalid/deckcraft/{ENGINE_REVISION}/");
        let mut candidate = fixture(HTML);
        candidate["runtimePackage"]["module"] = json!({
            "url":format!("{directory}deckcraft-web-4576e715d9c77da4.js"),
            "bytes":157851,
            "sha256":"7144d8766ed42db97e9f8366515328c7ef353a04d80d66c213ae107619c12ead",
            "mime":"text/javascript"
        });
        candidate["runtimePackage"]["wasm"] = json!({
            "url":format!("{directory}deckcraft-web-b023d45fa5023a1a_bg.wasm"),
            "bytes":36382474,
            "sha256":"c9ff98889b779e3c11661e290c1c74317fd6aa9eecbe7705c085f7ff74cd4c0e",
            "mime":"application/wasm"
        });
        assert!(valid_resource(HTML, &candidate.to_string()));
        let previous_touch_module = json!({
            "url":format!("{directory}deckcraft-web-9848d4c343e786ef.js"),
            "bytes":157851,
            "sha256":"9dd3081704ac39c04a8f78e56ccbf904b032bd5569186ce1df3cd6e0ebd88c77",
            "mime":"text/javascript"
        });
        let previous_touch_wasm = json!({
            "url":format!("{directory}deckcraft-web-55e43aa6139c5bb4_bg.wasm"),
            "bytes":36385233,
            "sha256":"cd911a688e3a6793cc0dbbab14a824646a7732a4391fdf50c976f28ed8e1d2d5",
            "mime":"application/wasm"
        });
        let previous_module = json!({
            "url":format!("{directory}deckcraft-web-1eb288115950a000.js"),
            "bytes":157851,
            "sha256":"4a3a583c958a150900c58e773c2ed58df6213e733ed3496e9c60c173cd9bebc4",
            "mime":"text/javascript"
        });
        let reproduced_wasm = json!({
            "url":format!("{directory}deckcraft-web-b257808a686003f1_bg.wasm"),
            "bytes":36387042,
            "sha256":"cb0af84197762083dc06f9b3812b6c60fe2bbbfeac534ff44c186081dd37b241",
            "mime":"application/wasm"
        });
        let missing_original_wasm = json!({
            "url":format!("{directory}deckcraft-web-41f0085af1d7648c_bg.wasm"),
            "bytes":36387050,
            "sha256":"2c6ab9025a157f346849dcbb4e7cc956e2afbb6665048727654f60995eee24c6",
            "mime":"application/wasm"
        });
        for (previous_module, previous_wasm) in [
            (previous_touch_module, previous_touch_wasm),
            (previous_module.clone(), reproduced_wasm),
            (previous_module, missing_original_wasm),
        ] {
            for (module, wasm) in [
                (previous_module.clone(), previous_wasm.clone()),
                (
                    previous_module.clone(),
                    candidate["runtimePackage"]["wasm"].clone(),
                ),
                (
                    candidate["runtimePackage"]["module"].clone(),
                    previous_wasm.clone(),
                ),
            ] {
                let mut retired = candidate.clone();
                retired["runtimePackage"]["module"] = module;
                retired["runtimePackage"]["wasm"] = wasm;
                assert!(!valid_resource(HTML, &retired.to_string()));
            }
            // A mixed descriptor must not borrow even one retired identity field.
            for (asset, old) in [("module", &previous_module), ("wasm", &previous_wasm)] {
                for field in ["url", "bytes", "sha256"] {
                    if candidate["runtimePackage"][asset][field] == old[field] {
                        continue;
                    }
                    let mut mixed = candidate.clone();
                    mixed["runtimePackage"][asset][field] = old[field].clone();
                    assert!(!valid_resource(HTML, &mixed.to_string()), "{asset}/{field}");
                }
            }
        }
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
