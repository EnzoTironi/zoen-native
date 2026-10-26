use super::*;
use std::{path::PathBuf, sync::Arc};
const SECRET: &str = "PRIVATE_PAYLOAD_TOKEN\nprovider_error/persona/history";
fn call(name: &str) -> ToolCall {
    ToolCall {
        id: SECRET.into(),
        name: name.into(),
        arguments: json!({"payload":SECRET}),
        requires_review: true,
    }
}
fn tool(name: &str) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: SECRET.into(),
        parameters: json!({"private":SECRET}),
    }
}

// Synthetic native-discovery bookkeeping, never an execution permit. The same
// production classifier checks exact item membership and generated model name.
struct Catalog {
    client: Arc<client_core::Client>,
    journal: PathBuf,
}
impl Catalog {
    fn new() -> Self {
        Self {
            client: client_core::Client::open(":memory:".into(), "en".into()).unwrap(),
            journal: PathBuf::from("unused-observation-journal"),
        }
    }
    fn host(&self) -> SpaceTools<'_> {
        let deck = format!("it_deck_{}", "a1".repeat(32));
        let page = "it_synthetic_page".to_string();
        let names = vec![
            (
                tool(&crate::deckcraft::model_name("read_deckcraft", &deck).unwrap()),
                deck.clone(),
                "read_deckcraft".into(),
            ),
            (
                tool(
                    &client_core::agent_page_tools::model_name("replace_page_block", &page)
                        .unwrap(),
                ),
                page.clone(),
                "replace_page_block".into(),
            ),
        ];
        SpaceTools {
            e: &self.client,
            journal: &self.journal,
            source: "synthetic-source",
            space: "synthetic-space".into(),
            serving: "synthetic-member",
            names,
            deckcraft: None,
            deckcraft_items: vec![deck],
            selection: None,
            page_items: vec![page],
            scene_items: vec![],
        }
    }
}
#[test]
fn default_off_and_invalid_bounds_emit_nothing_without_clamping() {
    let fixture = Catalog::new();
    let host = fixture.host();
    for value in [None, Some(""), Some("true"), Some("0"), Some("01")] {
        assert!(!enabled(value));
    }
    assert!(enabled(Some("1")));
    assert!(input_record(false, "space", "source", &[], &[], 0, &host).is_none());
    assert!(result_record(
        false,
        "space",
        "source",
        &call(SECRET),
        ToolObservation::HostError,
        &host
    )
    .is_none());
    assert!(input_record(
        true,
        "space",
        "source",
        &vec![tool(SECRET); 129],
        &[],
        0,
        &host
    )
    .is_none());
    assert!(input_record(true, "space", "source", &[], &[], 5, &host).is_none());
    let step = ToolStep {
        call: call(SECRET),
        result: SECRET.into(),
        is_error: true,
    };
    assert!(input_record(true, "space", "source", &[], &vec![step; 17], 0, &host).is_none());
    assert!(!fixture.journal.exists());
}

#[test]
fn scene_observation_requires_native_membership_and_keeps_payloads_private() {
    let fixture = Catalog::new();
    let mut host = fixture.host();
    let item = "it_synthetic_owned_scene".to_string();
    let name = client_core::agent_scenes::tools::model_name("replace_scene", &item).unwrap();
    host.names
        .push((tool(&name), item.clone(), "replace_scene".into()));
    assert!(public_kind(&host, &name).is_none());
    host.scene_items.push(item);
    let record = result_record(
        true,
        "space",
        "source",
        &call(&name),
        ToolObservation::Held,
        &host,
    )
    .unwrap();
    assert_eq!(record["tool"]["kind"], "replace_scene");
    assert_eq!(record["tool"]["artifactAlias"].as_str().unwrap().len(), 16);
    assert!(!record.to_string().contains("PRIVATE"));
}
#[test]
fn bound_offer_metadata_and_step_classes_never_disclose_payloads_or_unknown_names() {
    let fixture = Catalog::new();
    let host = fixture.host();
    let mut tools: Vec<_> = host.names.iter().map(|(def, _, _)| def.clone()).collect();
    tools.push(tool(SECRET));
    let steps = [
        ToolStep {
            call: call(SECRET),
            result: SECRET.into(),
            is_error: false,
        },
        ToolStep {
            call: call(SECRET),
            result: SECRET.into(),
            is_error: true,
        },
    ];
    let v = input_record(true, "space", "source", &tools, &steps, 2, &host).unwrap();
    assert_eq!(v["offeredCount"], 3);
    assert_eq!(v["unclassifiedCount"], 1);
    assert_eq!(v["completedSteps"], 2);
    assert_eq!(v["failedSteps"], 1);
    let deck = tools[0].name.strip_prefix("read_deckcraft__").unwrap();
    let page = tools[1].name.strip_prefix("replace_page_block_").unwrap();
    assert_eq!(
        v["recognizedTools"],
        json!([{"kind":"read_deckcraft","artifactAlias":deck},{"kind":"replace_page_block","artifactAlias":page}])
    );
    assert!(!v.to_string().contains("PRIVATE") && !v.to_string().contains("persona"));
    assert_eq!(v["correlation"], json!(correlate("space", "source")));
    let last = input_record(true, "space", "source", &[], &steps, 4, &host).unwrap();
    assert_eq!(last["offeredCount"], 0);
    assert_eq!(last["completedSteps"], 2);
    assert!(!fixture.journal.exists());
}
#[test]
fn unbound_generic_and_unknown_lookalikes_never_become_artifact_aliases() {
    let fixture = Catalog::new();
    let mut host = fixture.host();
    let page_lookalike = "read_page_0123456789abcdef";
    let deck_lookalike = format!("read_deckcraft__{}", "09".repeat(20));
    let unbound = [page_lookalike, deck_lookalike.as_str(), "read_deckcraft"];
    // Even offered generic App names cannot assert membership in a native catalog.
    for name in unbound {
        host.names
            .push((tool(name), "generic-app-item".into(), "read_page".into()));
        assert!(public_kind(&host, name).is_none());
        let result = result_record(
            true,
            "space",
            "source",
            &call(name),
            ToolObservation::HostOk,
            &host,
        )
        .unwrap();
        assert_eq!(result["tool"], json!({"kind":"other"}));
        assert!(
            !result.to_string().contains("0123456789abcdef")
                && !result.to_string().contains(&"09".repeat(20))
        );
    }
    // Native item membership still cannot legitimize a mismatching generated name.
    host.names.push((
        tool("replace_page_block_0123456789abcdef"),
        host.page_items[0].clone(),
        "replace_page_block".into(),
    ));
    assert!(public_kind(&host, "replace_page_block_0123456789abcdef").is_none());
    let offered: Vec<_> = host.names.iter().map(|(def, _, _)| def.clone()).collect();
    let input = input_record(true, "space", "source", &offered, &[], 0, &host).unwrap();
    assert_eq!(input["offeredCount"], 6);
    assert_eq!(input["unclassifiedCount"], 4);
    assert_eq!(input["recognizedTools"].as_array().unwrap().len(), 2);
    assert!(
        !input.to_string().contains("0123456789abcdef")
            && !input.to_string().contains(&"09".repeat(20))
    );
    // The Runtime's UnknownTool class always stays other, even if the host has
    // a native binding with that name (for example an exhausted offer boundary).
    let native = call(&host.names[0].0.name);
    let result = result_record(
        true,
        "space",
        "source",
        &native,
        ToolObservation::UnknownTool,
        &host,
    )
    .unwrap();
    assert_eq!(result["tool"], json!({"kind":"other"}));
    assert!(public_kind(&host, &native.name).is_some());
    assert!(!fixture.journal.exists());
}
#[test]
fn every_result_class_is_closed_and_failed_writers_do_not_panic_or_retry() {
    let fixture = Catalog::new();
    let host = fixture.host();
    struct Broken(usize);
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            Err(io::Error::other("PRIVATE raw IO"))
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("no flush or retry")
        }
    }
    for (outcome, class) in [
        (ToolObservation::Held, "held"),
        (ToolObservation::Blocked, "blocked"),
        (ToolObservation::UnknownTool, "unknown_tool"),
        (ToolObservation::PreparationRefused, "preparation_refused"),
        (ToolObservation::AdmissionError, "admission_error"),
        (ToolObservation::HostOk, "host_ok"),
        (ToolObservation::HostError, "host_error"),
    ] {
        let v = result_record(true, "space", "source", &call(SECRET), outcome, &host).unwrap();
        assert_eq!(v["resultClass"], class);
        assert_eq!(v["tool"], json!({"kind":"other"}));
        assert!(!v.to_string().contains("PRIVATE"));
        let mut writer = Broken(0);
        emit_to(&v, &mut writer);
        assert_eq!(writer.0, 1);
    }
}
#[test]
fn maximum_catalog_is_bounded_and_keeps_every_offered_entry() {
    let fixture = Catalog::new();
    let host = fixture.host();
    let tools = vec![host.names[0].0.clone(); 128];
    let v = input_record(true, "space", "source", &tools, &[], 0, &host).unwrap();
    assert_eq!(v["recognizedTools"].as_array().unwrap().len(), 128);
    assert_eq!(v["offeredCount"], 128);
    assert!(v.to_string().len() < 16 * 1024);
}
