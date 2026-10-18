use roda_apps::{
    create_as, headline, snapshot, spec, spec_for_start_tool, spec_for_uri, specs, view_for, Actor,
};
use roda_i18n::{set, Lang};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[test]
fn portable_catalog_keeps_resource_integrity_and_instance_identity() {
    let actor = Actor {
        id: "person",
        name: "Person",
        owner: None,
    };
    for language in [Lang::En, Lang::Pt] {
        set(language);
        let catalog = specs();
        assert_eq!(catalog.len(), 8);
        for entry in catalog {
            let start = entry
                .tools
                .iter()
                .find(|tool| tool.visibility.contains(&"model"))
                .unwrap();
            assert_eq!(spec_for_start_tool(start.name).unwrap().id, entry.id);
            assert_eq!(spec_for_uri(entry.resource_uri).unwrap().id, entry.id);
            assert_eq!(spec(entry.id).unwrap().resource_uri, entry.resource_uri);
            let args = match entry.id {
                "poll" => json!({ "question": "Where?", "options": ["Park", "Beach"] }),
                "countdown" => json!({ "title": "Trip", "target_ms": 2_000_000 }),
                _ => json!({}),
            };
            let (title, state) = create_as(entry.id, &args, &actor, 1_000_000).unwrap();
            assert!(!title.is_empty(), "{}", entry.id);
            assert!(view_for(entry.id, &state, 1_000_000, actor.id).is_object());
            assert!(!headline(entry.id, &state, 1_000_000).0.is_empty());
            let widget = snapshot(entry.id, &state, "it_portable", 1_000_000).unwrap();
            assert_eq!(widget["deepLink"], "zoen://app/it_portable");
            if let Some(manifest) = entry.manifest {
                let manifest: Value = serde_json::from_str(manifest).unwrap();
                let html = entry.html.expect("manifest requires an embedded resource");
                assert_eq!(
                    manifest["sha256"],
                    format!("{:x}", Sha256::digest(html.as_bytes()))
                );
            }
        }
    }
}

#[test]
fn native_and_portable_rules_use_the_same_language_context() {
    set(Lang::Pt);
    assert_eq!(spec("pet").unwrap().name, "Jumento do grupo");
    assert!(create_as(
        "poll",
        &json!({}),
        &Actor {
            id: "p",
            name: "P",
            owner: None
        },
        0
    )
    .unwrap_err()
    .contains("pergunta"));
    set(Lang::En);
    assert_eq!(spec("pet").unwrap().name, "Group donkey");
    assert!(create_as(
        "poll",
        &json!({}),
        &Actor {
            id: "p",
            name: "P",
            owner: None
        },
        0
    )
    .unwrap_err()
    .contains("question"));
}
