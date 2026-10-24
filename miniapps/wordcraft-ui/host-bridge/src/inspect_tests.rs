//! Bound the actual serialized envelope, not character estimates or a truncated model.
use super::*;
fn inspect_text(text: &str) -> Result<(HostBridge, Vec<u8>, Value)> {
    let (bridge, bytes) = opened(text)?;
    let value = bridge.inspect()?;
    let envelope = serde_json::to_vec(&json!({"ok":true,"metadata":value}))
        .map_err(|_| failure("fixture_json"))?;
    assert!(envelope.len() <= 64 * 1024);
    assert_eq!(value["text"], text);
    assert_eq!(exported(&bridge, head("1", &bytes))?.bytes, bytes);
    Ok((bridge, bytes, value))
}
#[test]
fn full_32k_legacy_reads_reference_one_utf8_text_and_count_omitted_rich_details() -> Result<()> {
    for text in [
        "a".repeat(profile::MAX_TEXT),
        "á".repeat(profile::MAX_TEXT / 2),
        "漢\"\\\t".repeat(1000),
    ] {
        let (_, _, v) = inspect_text(&text)?;
        assert_eq!(v["richDetails"]["omitted"], false);
        let p = &v["paragraphs"][0];
        assert!(p.get("text").is_none());
        assert_eq!(p["textRange"]["start"], 0);
    }
    let text = vec!["a"; 1024].join("\n");
    let (bridge, _, v) = inspect_text(&text)?;
    assert_eq!(v["richDetails"]["omitted"], true);
    assert_eq!(v["richDetails"]["reason"], "metadata_limit");
    assert_eq!(v["richDetails"]["bodyParagraphs"], 1024);
    assert!(v.get("styles").is_none() && v.get("paragraphs").is_none());
    assert_eq!(bridge.status()["pendingMutationId"], Value::Null);
    Ok(())
}
#[test]
fn escaping_that_cannot_fit_the_wire_refuses_without_losing_the_accepted_document() -> Result<()> {
    let text = "\\\"".repeat(profile::MAX_TEXT / 2);
    let (bridge, bytes) = opened(&text)?;
    assert_eq!(
        bridge.inspect().err(),
        Some(failure("inspection_metadata_limit"))
    );
    assert_eq!(exported(&bridge, head("1", &bytes))?.bytes, bytes);
    assert_eq!(profile::parse(&bytes)?.plain_text(StoryRef::Body), text);
    assert_eq!(bridge.status()["pendingMutationId"], Value::Null);
    Ok(())
}
