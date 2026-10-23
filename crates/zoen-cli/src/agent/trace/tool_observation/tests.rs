use super::*;
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
#[test]
fn default_off_and_invalid_bounds_emit_nothing_without_clamping() {
    for value in [None, Some(""), Some("true"), Some("0"), Some("01")] {
        assert!(!enabled(value));
    }
    assert!(enabled(Some("1")));
    assert!(input_record(false, "space", "source", &[], &[], 0).is_none());
    assert!(result_record(
        false,
        "space",
        "source",
        &call(SECRET),
        ToolObservation::HostError
    )
    .is_none());
    assert!(input_record(true, "space", "source", &vec![tool(SECRET); 129], &[], 0).is_none());
    assert!(input_record(true, "space", "source", &[], &[], 5).is_none());
    let step = ToolStep {
        call: call(SECRET),
        result: SECRET.into(),
        is_error: true,
    };
    assert!(input_record(true, "space", "source", &[], &vec![step; 17], 0).is_none());
}
#[test]
fn actual_offer_metadata_and_step_classes_never_disclose_payloads_or_unknown_names() {
    let alias = "ab".repeat(20);
    let page = "cd".repeat(8);
    let tools = [
        tool(&format!("read_deckcraft__{alias}")),
        tool(&format!("replace_page_block_{page}")),
        tool(SECRET),
    ];
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
    let v = input_record(true, "space", "source", &tools, &steps, 2).unwrap();
    assert_eq!(v["offeredCount"], 3);
    assert_eq!(v["unclassifiedCount"], 1);
    assert_eq!(v["completedSteps"], 2);
    assert_eq!(v["failedSteps"], 1);
    assert_eq!(
        v["recognizedTools"],
        json!([{"kind":"read_deckcraft","artifactAlias":alias},{"kind":"replace_page_block","artifactAlias":page}])
    );
    assert!(!v.to_string().contains("PRIVATE") && !v.to_string().contains("persona"));
    assert_eq!(v["correlation"], json!(correlate("space", "source")));
    let last = input_record(true, "space", "source", &[], &steps, 4).unwrap();
    assert_eq!(last["offeredCount"], 0);
    assert_eq!(last["completedSteps"], 2);
}
#[test]
fn every_result_class_is_closed_and_failed_writers_do_not_panic_or_retry() {
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
        let v = result_record(true, "space", "source", &call(SECRET), outcome).unwrap();
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
    let tools = vec![tool(&format!("deckcraft_shape_insert__{}", "ab".repeat(20))); 128];
    let v = input_record(true, "space", "source", &tools, &[], 0).unwrap();
    assert_eq!(v["recognizedTools"].as_array().unwrap().len(), 128);
    assert_eq!(v["offeredCount"], 128);
    assert!(v.to_string().len() < 16 * 1024);
}
