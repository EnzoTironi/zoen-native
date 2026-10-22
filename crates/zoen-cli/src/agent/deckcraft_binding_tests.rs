use super::*;
#[test]
fn unoffered_binary_binding_cannot_replay_a_cached_success_without_authority() {
    let e = Client::open(":memory:".into(), "en".into()).unwrap();
    let dir = std::env::temp_dir().join(types::new_id("worker-unoffered"));
    std::fs::create_dir_all(&dir).unwrap();
    let journal = dir.join("steps.json");
    let item = format!("it_deck_{}", "ab".repeat(32));
    let call = ToolCall {
        id: "retained".into(),
        name: crate::deckcraft::model_name("deckcraft_text_set", &item).unwrap(),
        arguments: json!({"artifactId":item,"version":1,"sha256":"00".repeat(32),"slideId":257,"id":258,"text":"retained"}),
    };
    let host = SpaceTools {
        selection: None,
        e: &e,
        journal: &journal,
        source: "prior-source",
        space: "synthetic-space".into(),
        serving: "prior-member",
        names: vec![],
        deckcraft: None,
        deckcraft_items: vec![],
    };
    host.record(ToolStep {
        call: call.clone(),
        result: json!({"itemId":item,"version":2,"replayed":false}).to_string(),
        is_error: false,
    });
    let before = std::fs::read(&journal).unwrap();
    assert_eq!(host.done().len(), 1);
    assert!(host.call(&call).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn runtime_public_references_preserve_gate_on_semantics_and_exact_mismatch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
    struct CheckedHost {
        definition: ToolDef,
        references: Option<zoen_agentd::PublicArtifactReferences>,
        calls: AtomicUsize,
    }
    impl ToolHost for CheckedHost {
        fn tools(&self) -> Vec<ToolDef> {
            vec![self.definition.clone()]
        }
        fn gate_public_references(
            &self,
            _: &ToolCall,
        ) -> Option<zoen_agentd::PublicArtifactReferences> {
            self.references.clone()
        }
        fn call(&self, _: &ToolCall) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok("accepted".into())
        }
    }
    let item = format!("it_deck_{}", "a1".repeat(32));
    let sha = "c2".repeat(32);
    let name = crate::deckcraft::model_name("deckcraft_slide_notes", &item).unwrap();
    let base = json!({"artifactId":item,"version":1,"sha256":sha,"slideId":257,"text":"Reviewed presentation notes"});
    let definition: Definition = serde_json::from_value(
        json!({"version":1,"kind":"user","persona":"Edit reviewed notes","rules":[],"state":{}}),
    )
    .unwrap();
    let cases = [
        (
            "exact public references",
            base.clone(),
            "Presentation notes".to_string(),
            true,
            1,
        ),
        (
            "default raw call still blocks",
            base.clone(),
            "Presentation notes".to_string(),
            false,
            0,
        ),
        (
            "payload secret retained",
            {
                let mut a = base.clone();
                a["text"] = format!("sk-test-{}", "a1".repeat(24)).into();
                a
            },
            "Presentation notes".to_string(),
            true,
            0,
        ),
        (
            "destructive payload retained",
            {
                let mut a = base.clone();
                a["text"] = "rm -rf ~/Documents".into();
                a
            },
            "Presentation notes".to_string(),
            true,
            0,
        ),
        (
            "title retained",
            base.clone(),
            format!("Presentation sk-test-{}", "a1".repeat(24)),
            true,
            0,
        ),
        (
            "wrong artifact not stripped",
            {
                let mut a = base.clone();
                a["artifactId"] = format!("it_deck_{}", "e3".repeat(32)).into();
                a
            },
            "Presentation notes".to_string(),
            true,
            0,
        ),
        (
            "wrong hash not stripped",
            {
                let mut a = base.clone();
                a["sha256"] = "e3".repeat(32).into();
                a
            },
            "Presentation notes".to_string(),
            true,
            0,
        ),
        (
            "nested public-looking payload retained",
            {
                let mut a = base.clone();
                a["text"] = format!("Reference {}", item).into();
                a
            },
            "Presentation notes".to_string(),
            true,
            0,
        ),
    ];
    for (label, arguments, description, use_refs, expected_calls) in cases {
        let host = CheckedHost {
            definition: ToolDef {
                name: name.clone(),
                description,
                parameters: json!({}),
            },
            references: use_refs.then(|| {
                zoen_agentd::PublicArtifactReferences::new(
                    "deckcraft_slide_notes".into(),
                    item.clone(),
                    Some(sha.clone()),
                )
                .unwrap()
            }),
            calls: AtomicUsize::new(0),
        };
        let response = format!(
            "```tools\n{}\n```\nApplied notes.",
            json!([{"name":name,"arguments":arguments}])
        );
        let runtime = Runtime::new(
            Gate::default(),
            Box::new(ScriptedBrain {
                replies: [("reviewed edit".into(), response)].into(),
            }),
        );
        let inbound = Turn {
            space: "space".into(),
            from: "owner".into(),
            text: "Update presentation notes with the reviewed edits".into(),
        };
        let outcome = runtime
            .turn_with_tools(
                TurnContext {
                    agent: "agent",
                    device: "device",
                    owner: "owner",
                    definition: &definition,
                },
                &[],
                &inbound,
                &host,
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(
            host.calls.load(Ordering::SeqCst),
            expected_calls,
            "{label}: {outcome:?}"
        );
        if expected_calls == 1 {
            assert_eq!(
                outcome,
                TurnOutcome::Say {
                    text: "Applied notes.".into()
                },
                "{label}"
            );
        }
    }
}
