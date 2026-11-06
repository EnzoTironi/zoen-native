use super::*;

#[test]
fn scoped_reference_retains_its_signed_wire_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let original = concat!(
        r#"{"to":"root","thread":true,"artifact":{"item_id":"item","version":1,"sha256":""#,
        "0000000000000000000000000000000000000000000000000000000000000000",
        r#"","request_id":"request","text_sha256":""#,
        "1111111111111111111111111111111111111111111111111111111111111111",
        r#""}}"#,
    );
    let reply: ReplyRef = serde_json::from_str(original)?;
    assert!(reply
        .artifact
        .as_ref()
        .is_some_and(|request| request.is_well_formed()));
    assert_eq!(serde_json::to_string(&reply)?, original);
    Ok(())
}

#[test]
fn old_thread_references_keep_their_exact_wire_shape() -> Result<(), Box<dyn std::error::Error>> {
    let original = r#"{"to":"root","thread":true}"#;
    let reply: ReplyRef = serde_json::from_str(original)?;
    assert_eq!(reply, ReplyRef::thread("root"));
    assert_eq!(reply.source_id(), "root");
    assert_eq!(serde_json::to_string(&reply)?, original);
    Ok(())
}

#[test]
fn exact_source_round_trips_without_replacing_the_thread_root(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reply = ReplyRef::thread("root");
    reply.source = Some("child".into());
    let recovered: ReplyRef = serde_json::from_slice(&serde_json::to_vec(&reply)?)?;
    assert_eq!(recovered.to, "root");
    assert_eq!(recovered.source_id(), "child");
    assert!(recovered.thread);
    // Older explicit selections also retain their exact answered message.
    reply.source = None;
    reply.quote = Some(Box::new(SelectedQuote {
        source_id: "child".into(),
        revision: "revision".into(),
        text: "literal".into(),
        spans: vec![crate::QuoteSpan {
            start_utf16: 0,
            end_utf16: 7,
        }],
    }));
    assert_eq!(reply.source_id(), "child");
    Ok(())
}
