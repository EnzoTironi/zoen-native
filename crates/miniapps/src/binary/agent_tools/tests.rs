use super::*;
use serde_json::json;
#[test]
fn grid_tools_require_exact_head_and_closed_cell_command_without_actor_authority() {
    let args = json!({"artifactId":"it_grid_abc","version":7,"sha256":"a".repeat(64),"sheet":"Sheet1","cell":"B2","input":"=A1*2"});
    let (version, hash, action) = command("gridcraft_cell_set", &args).expect("valid cell command");
    assert_eq!(version, Some(7));
    assert_eq!(hash, Some("a".repeat(64)));
    assert_eq!(
        action,
        Some(json!({"command":"cell.set","params":{"sheet":"Sheet1","cell":"B2","input":"=A1*2"}}))
    );
    for (key, value) in [
        ("actorId", json!("forged")),
        ("scope", json!({})),
        ("path", json!("/tmp/doc")),
        ("source", json!("msg")),
    ] {
        let mut bad = args.clone();
        bad[key] = value;
        assert!(command("gridcraft_cell_set", &bad).is_err());
    }
    for (key, value) in [
        ("version", json!(0)),
        ("sha256", json!("A".repeat(64))),
        ("cell", json!("A10001")),
        ("cell", json!("XFD1")),
        ("input", json!("=WEBSERVICE(\"https://example.invalid\")")),
        ("sheet", json!("")),
    ] {
        let mut bad = args.clone();
        bad[key] = value;
        assert!(command("gridcraft_cell_set", &bad).is_err(), "{key}");
    }
    assert!(command("gridcraft_cell_set", &json!({"artifactId":"it_grid_abc"})).is_err());
    assert_eq!(
        command("read_gridcraft", &json!({"artifactId":"it_grid_abc"})).expect("read"),
        (None, None, None)
    );
    assert!(command("read_gridcraft", &args).is_err());
    assert!(command("deckcraft_text_set", &args).is_err());
}
#[test]
fn binary_dispatch_keeps_deck_aliases_and_grid_artifact_bindings_distinct() {
    let item = format!("it_grid_{}", "a".repeat(64));
    let grid = model_name("gridcraft_cell_set", &item).expect("bounded alias");
    assert!(grid.len() <= 64);
    assert!(is_model_name(&grid));
    assert_ne!(model_name("gridcraft_cell_set", "other"), Some(grid));
    assert_eq!(
        model_name("read_deckcraft", &item),
        deckcraft::agent_tools::model_name("read_deckcraft", &item)
    );
    assert_eq!(format("read_gridcraft"), Some(Format::Gridcraft));
    assert_eq!(format("read_deckcraft"), Some(Format::Deckcraft));
    assert!(!recognizes("gridcraft_eval"));
    assert!(model_name("gridcraft_eval", &item).is_none());
    assert!(is_read("read_gridcraft"));
    assert!(!is_read("gridcraft_cell_set"));
}
