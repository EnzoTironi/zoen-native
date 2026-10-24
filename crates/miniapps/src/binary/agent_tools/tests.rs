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

#[test]
fn word_tools_keep_exact_head_text_profile_and_actor_binding_closed() {
    let good = json!({"artifactId":"it_word_abc","version":3,"sha256":"a".repeat(64),"text":"Olá 日本語\n<&>\t"});
    assert_eq!(
        command("wordcraft_text_replace", &good).expect("closed text replacement"),
        (
            Some(3),
            Some("a".repeat(64)),
            Some(json!({"command":"text.replace","params":{"text":"Olá 日本語\n<&>\t"}}))
        )
    );
    for (key, value) in [
        ("actorId", json!("forged")),
        ("scope", json!({})),
        ("path", json!("/tmp/x")),
        ("version", json!(0)),
        ("sha256", json!("A".repeat(64))),
        ("text", json!("\u{fffe}")),
        ("text", json!("a".repeat(16385))),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(command("wordcraft_text_replace", &bad).is_err(), "{key}");
    }
    assert_eq!(
        command("read_wordcraft", &json!({"artifactId":"it_word_abc"})).expect("read"),
        (None, None, None)
    );
    let mut escaped = good.clone();
    escaped["text"] = json!("\"".repeat(16384));
    assert!(
        command("wordcraft_text_replace", &escaped).is_err(),
        "JSON escaping still respects the existing signed envelope limit"
    );
    assert!(command("read_wordcraft", &good).is_err());
    assert!(command(
        "wordcraft_text_replace",
        &json!({"artifactId":"it_word_abc"})
    )
    .is_err());
    assert!(is_read("read_wordcraft"));
    assert!(!is_read("wordcraft_text_replace"));
    let alias = model_name("wordcraft_text_replace", "it_word_abc").expect("bounded alias");
    assert!(is_model_name(&alias));
    assert!(alias.len() <= 64);
    assert_ne!(Some(alias), model_name("wordcraft_text_replace", "other"));
    assert!(!recognizes("wordcraft_command"));
}

#[test]
fn rich_word_range_tools_bind_exact_styles_and_reject_ambiguous_commands() -> Result<(), String> {
    let range = json!({"start":{"paragraph":0,"offset":0},"end":{"paragraph":0,"offset":4}});
    let mut format = json!({"artifactId":"it_word_abc","version":3,"sha256":"a".repeat(64),"range":range,"format":{"bold":true,"italic":false,"size":18,"alignment":"center"}});
    let (_, _, action) = command("wordcraft_format_range", &format)?;
    assert_eq!(
        action,
        Some(
            json!({"command":"range.format","params":{"range":range,"format":{"bold":true,"italic":false,"size":18.0,"alignment":"center"}}})
        )
    );
    let alias =
        model_name("wordcraft_format_range", "it_word_abc").ok_or("missing bounded alias")?;
    assert_eq!(alias.len(), 64);
    assert!(is_model_name(&alias));
    assert!(!is_read("wordcraft_format_range"));
    for bad in [
        json!({}),
        json!({"bold":null}),
        json!({"bold":true,"path":"/tmp/doc"}),
        json!({"size":18.1}),
        json!({"color":"#fff"}),
        json!({"lineSpacing":0}),
        json!({"alignment":"invalid"}),
    ] {
        format["format"] = bad;
        assert!(command("wordcraft_format_range", &format).is_err());
    }
    format["format"] = json!({"bold":true});
    for bad in [
        json!({"start":{"paragraph":0,"offset":2},"end":{"paragraph":0,"offset":1}}),
        json!({"start":{"paragraph":0,"offset":0},"end":{"paragraph":0,"offset":0}}),
        json!({"start":{"paragraph":-1,"offset":0},"end":{"paragraph":0,"offset":1}}),
    ] {
        format["range"] = bad;
        assert!(command("wordcraft_format_range", &format).is_err());
    }
    let edit = json!({"artifactId":"it_word_abc","version":3,"sha256":"a".repeat(64),"range":range,"text":"New text\nSecond paragraph"});
    let (_, _, action) = command("wordcraft_edit_range", &edit)?;
    assert_eq!(
        action,
        Some(
            json!({"command":"text.edit","params":{"range":range,"text":"New text\nSecond paragraph"}})
        )
    );
    assert!(command("wordcraft_format_range", &edit).is_err());
    assert!(command("gridcraft_cell_set", &edit).is_err());
    Ok(())
}
