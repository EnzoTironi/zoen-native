//! Model-wire outcomes are closed; native semantics still validate every call.
use super::*;

fn proposal(kind: &str, calls: Value) -> String {
    json!({"outcome":{"kind":kind,"calls":calls}}).to_string()
}
fn call(id: &str, name: &str, arguments: &str) -> Value {
    json!({"id":id,"name":name,"arguments_json":arguments})
}
fn parse(value: Value, tools: &[ToolDef]) -> Result<BrainOut, String> {
    parse_output(&value.to_string(), tools)
}

#[test]
fn nested_outcome_accepts_only_complete_individual_variants() {
    for text in ["", "  ", " A plain answer ",
        "Ready.\n```json\n{\"create_agent\":{\"name\":\"New agent\",\"handle\":\"new_agent\",\"persona\":\"Help\"}}\n```"] {
        let actual = parse(json!({"outcome":{"kind":"say","text":text}}), &[]).unwrap();
        assert_eq!(actual, parse_reply(text), "legacy say interpretation stays exact");
    }
    assert_eq!(
        parse(json!({"outcome":{"kind":"quiet"}}), &[]).unwrap(),
        BrainOut::Silence
    );
    for (kind, required) in [("call", false), ("review", true)] {
        let value = proposal(kind, json!([call("id-1", "edit_deck", "{\"x\":1}")]));
        let BrainOut::CallTools(calls) = parse_output(&value, &[tool()]).unwrap() else {
            panic!("expected proposed calls");
        };
        assert_eq!(
            calls,
            vec![ToolCall {
                id: "id-1".into(),
                name: "edit_deck".into(),
                arguments: json!({"x":1}),
                requires_review: required
            }]
        );
    }
}

#[test]
fn nested_contract_refuses_old_flat_shape_mixed_outcomes_and_unknown_fields() {
    for value in [
        json!({"kind":"say","text":"old flat","calls":[]}),
        json!({"kind":"quiet","text":"","calls":[]}),
        json!({"outcome":{"kind":"say","text":"mixed","calls":[]}}),
        json!({"outcome":{"kind":"quiet","text":""}}),
        json!({"outcome":{"kind":"quiet","calls":[]}}),
        json!({"outcome":{"kind":"quiet","requires_review":true}}),
        json!({"outcome":{"kind":"review","text":"mixed","calls":[call("one","edit_deck","{}")]}}),
        json!({"outcome":{"kind":"call","text":"","calls":[call("one","edit_deck","{}")]}}),
        json!({"outcome":{"kind":"say"}}),
        json!({"outcome":{"kind":"other"}}),
        json!({"outcome":{"kind":"quiet"},"text":"extra root"}),
        json!({"outcome":{"kind":"call","calls":[{"id":"one","name":"edit_deck","arguments_json":"{}","requires_review":false}]}}),
        json!({"outcome":null}),
        json!({}),
    ] {
        assert!(
            parse(value.clone(), &[tool()]).is_err(),
            "must refuse: {value}"
        );
    }
    for raw in [
        r#"{"outcome":{"kind":"quiet"},"outcome":{"kind":"say","text":"ambiguous"}}"#,
        r#"{"outcome":{"kind":"say","kind":"quiet","text":"ambiguous"}}"#,
        r#"{"outcome":{"kind":"say","text":"a","text":"b"}}"#,
        r#"{"outcome":{"kind":"call","calls":[],"calls":[]}}"#,
    ] {
        assert!(
            parse_output(raw, &[tool()]).is_err(),
            "duplicate field must refuse"
        );
    }
}

#[test]
fn nested_calls_keep_id_count_offering_json_object_and_review_boundaries() {
    for kind in ["call", "review"] {
        for count in [0, super::super::super::host::MAX_CALLS_PER_ROUND + 1] {
            let calls: Vec<Value> = (0..count)
                .map(|n| call(&format!("id{n}"), "edit_deck", "{}"))
                .collect();
            assert!(parse_output(&proposal(kind, json!(calls)), &[tool()]).is_err());
        }
        let calls: Vec<Value> = (0..super::super::super::host::MAX_CALLS_PER_ROUND)
            .map(|n| call(&format!("id{n}"), "edit_deck", "{}"))
            .collect();
        assert!(
            matches!(parse_output(&proposal(kind,json!(calls)), &[tool()]).unwrap(), BrainOut::CallTools(v) if v.len()==super::super::super::host::MAX_CALLS_PER_ROUND)
        );
        for id in [
            "".to_string(),
            "x".repeat(129),
            "í".into(),
            "has space".into(),
            "a/b".into(),
        ] {
            assert!(parse_output(
                &proposal(kind, json!([call(&id, "edit_deck", "{}")])),
                &[tool()]
            )
            .is_err());
        }
        for id in ["a".to_string(), "x".repeat(128), "A0_.-".into()] {
            assert!(parse_output(
                &proposal(kind, json!([call(&id, "edit_deck", "{}")])),
                &[tool()]
            )
            .is_ok());
        }
        assert!(parse_output(
            &proposal(
                kind,
                json!([
                    call("same", "edit_deck", "{}"),
                    call("same", "edit_deck", "{}")
                ])
            ),
            &[tool()]
        )
        .is_err());
        assert!(parse_output(
            &proposal(kind, json!([call("one", "unoffered", "{}")])),
            &[tool()]
        )
        .is_err());
        assert!(parse_output(
            &proposal(kind, json!([call("one", "edit_deck", "{}")])),
            &[]
        )
        .is_err());
        for arguments in ["[]", "null", "42", "\"text\"", "{invalid"] {
            assert!(parse_output(
                &proposal(kind, json!([call("one", "edit_deck", arguments)])),
                &[tool()]
            )
            .is_err());
        }
    }
}

async fn think_with_catalog(brain: &CodexBrain, tools: &[ToolDef]) -> Result<BrainOut, String> {
    let def = definition();
    let cx = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &def,
    };
    brain
        .think_with_tools(
            cx,
            &[],
            &Turn {
                space: "space".into(),
                from: "owner".into(),
                text: "Use the actual catalog".into(),
            },
            tools,
            &[],
        )
        .await
}

#[tokio::test]
async fn actual_wire_schema_has_only_closed_disjoint_shapes_and_actual_offered_names() {
    let mut second = tool();
    second.name = "another.tool-2".into();
    let catalog = [tool(), second, tool()];
    let (_dir, brain, marker) = fixture("nested-contract");
    let result = think_with_catalog(&brain, &catalog).await;
    let schema: Value =
        serde_json::from_slice(&std::fs::read(marker.with_extension("schema")).unwrap()).unwrap();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["outcome"]));
    assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
    let variants = schema["properties"]["outcome"]["anyOf"].as_array().unwrap();
    assert_eq!(variants.len(), 3);
    let mut kinds = std::collections::BTreeSet::new();
    for variant in variants {
        assert_eq!(variant["type"], "object");
        assert_eq!(variant["additionalProperties"], false);
        let properties = variant["properties"].as_object().unwrap();
        assert_eq!(
            variant["required"].as_array().unwrap().len(),
            properties.len()
        );
        for k in variant["properties"]["kind"]["enum"].as_array().unwrap() {
            kinds.insert(k.as_str().unwrap());
        }
        if let Some(calls) = properties.get("calls") {
            assert!(!properties.contains_key("text"));
            assert_eq!(properties.len(), 2);
            assert_eq!(calls["minItems"], 1);
            assert_eq!(
                calls["maxItems"],
                super::super::super::host::MAX_CALLS_PER_ROUND
            );
            let item = &calls["items"];
            assert_eq!(item["additionalProperties"], false);
            assert_eq!(item["required"], json!(["id", "name", "arguments_json"]));
            assert_eq!(
                item["properties"]["id"]["pattern"],
                "^[A-Za-z0-9_.-]{1,128}$"
            );
            let names = item["properties"]["name"]["enum"].as_array().unwrap();
            assert_eq!(names, &vec![json!("another.tool-2"), json!("edit_deck")]);
        } else if properties.contains_key("text") {
            assert_eq!(properties.len(), 2);
        } else {
            assert_eq!(properties.len(), 1);
        }
    }
    assert_eq!(
        kinds,
        std::collections::BTreeSet::from(["say", "quiet", "call", "review"])
    );
    assert!(
        matches!(result.unwrap(),BrainOut::CallTools(v) if v.len()==1 && v[0].name=="edit_deck")
    );
    println!("CODEX_OUTCOME_SCHEMA_WITH_TOOLS={schema}");
}

#[tokio::test]
async fn empty_offered_catalog_sends_only_say_or_quiet_shapes_and_accepts_quiet() {
    let (_dir, brain, marker) = fixture("nested-contract");
    let result = think_with_catalog(&brain, &[]).await;
    let schema: Value =
        serde_json::from_slice(&std::fs::read(marker.with_extension("schema")).unwrap()).unwrap();
    let variants = schema["properties"]["outcome"]["anyOf"].as_array().unwrap();
    assert_eq!(variants.len(), 2);
    for variant in variants {
        let kinds = variant["properties"]["kind"]["enum"].as_array().unwrap();
        assert!(kinds.iter().all(|v| v == "say" || v == "quiet"));
        assert!(!variant["properties"]
            .as_object()
            .unwrap()
            .contains_key("calls"));
    }
    assert_eq!(result.unwrap(), BrainOut::Silence);
    println!("CODEX_OUTCOME_SCHEMA_NO_TOOLS={schema}");
}
