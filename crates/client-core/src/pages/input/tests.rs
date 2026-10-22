use super::*;

fn convert(text: &str, inline: serde_json::Value, blocks: serde_json::Value) -> Vec<PageBlockDto> {
    let end = text.encode_utf16().count();
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema":"enrm-ios/1", "sequence":"1", "mode":"rich", "text":text,
        "selection":{"start":end,"end":end}, "composition":null,
        "inline":inline, "blocks":blocks, "typing":{"add":[],"remove":[]}
    }))
    .unwrap();
    let binding = Binding {
        writer_id: "semantic-test".into(),
        target: PageInputTarget::AppendNew,
        base_edit_context: String::new(),
        base_block: None,
        mode: PageInputMode::RawRich,
    };
    validate_payload(&binding.mode, 1, &payload).unwrap();
    let capture = Capture {
        sequence: 1,
        sha256: digest(&payload),
        payload,
    };
    page_edits(&binding, &capture).unwrap()
}

#[test]
fn native_rich_apply_preserves_soft_breaks_paragraphs_and_literal_text() {
    for (text, markdown) in [("a\nb", "a\nb"), ("a\n\nb", "a\n\nb")] {
        let actual = convert(text, serde_json::json!([]), serde_json::json!([]));
        let expected = super::super::parse_editor_markdown(markdown).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(&expected) {
            assert_eq!((&a.kind, &a.text, &a.spans), (&e.kind, &e.text, &e.spans));
        }
    }
    let strong = convert(
        "a\nb",
        serde_json::json!([
            {"start":0,"end":3,"type":"strong","url":null,"origin":"stored"}
        ]),
        serde_json::json!([]),
    );
    assert_eq!(strong.len(), 1);
    assert_eq!(strong[0].text, "a\u{2028}b");
    assert_eq!(
        strong[0].spans,
        vec![TextSpanDto {
            start: 0,
            end: 3,
            key: "b".into(),
            value: String::new()
        }]
    );
    let literal = convert(
        "**literal**\n# literal",
        serde_json::json!([]),
        serde_json::json!([]),
    );
    assert_eq!(literal.len(), 1);
    assert_eq!(literal[0].kind, "paragraph");
    assert_eq!(literal[0].text, "**literal**\u{2028}# literal");
    assert!(literal[0].spans.is_empty());
    let unicode = convert(
        "é\n😀\n\n雨",
        serde_json::json!([
            {"start":0,"end":4,"type":"strong","url":null,"origin":"stored"}
        ]),
        serde_json::json!([]),
    );
    assert_eq!(
        unicode.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        ["é\u{2028}😀", "雨"]
    );
    assert_eq!(unicode[0].spans[0].end, 4);
    let structured = convert(
        "Title\nOne\nTwo",
        serde_json::json!([]),
        serde_json::json!([
            {"start":0,"end":5,"type":2,"level":0,"ordinal":1},
            {"start":6,"end":9,"type":7,"level":1,"ordinal":1},
            {"start":10,"end":13,"type":8,"level":2,"ordinal":3}
        ]),
    );
    assert_eq!(
        structured
            .iter()
            .map(|b| b.kind.as_str())
            .collect::<Vec<_>>(),
        ["heading", "bullet", "numbered"]
    );
    assert_eq!(
        (
            structured[0].level,
            structured[1].indent,
            structured[2].indent,
            structured[2].number
        ),
        (2, 1, 2, 3)
    );
}
