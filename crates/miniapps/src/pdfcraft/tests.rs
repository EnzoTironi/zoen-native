use super::*;
use sha2::{Digest, Sha256};
const SOURCE: &[u8] = include_bytes!("fixtures/actual-source.pdf");
const ROTATED: &[u8] = include_bytes!("fixtures/actual-wasi-rotated.pdf");

#[test]
fn actual_writer_and_wasi_sources_pass_without_mutation() -> Result<(), &'static str> {
    for (bytes, pages) in [(EMPTY, 1), (SOURCE, 2), (ROTATED, 2)] {
        let before = Sha256::digest(bytes);
        assert_eq!(Document::parse(bytes)?.declared_pages, pages);
        assert_eq!(Sha256::digest(bytes), before);
    }
    assert!(Document::parse(SOURCE)?
        .metadata("Review")
        .contains("\"declaredPages\":2"));
    Ok(())
}

#[test]
fn malformed_ambiguous_and_oversized_sources_are_refused() {
    let mut prefixed = b"untrusted prefix".to_vec();
    prefixed.extend_from_slice(SOURCE);
    for bytes in [
        b"".as_slice(),
        b"%PDF-1.7\n%%EOF",
        &prefixed,
        &SOURCE[..SOURCE.len() - 10],
    ] {
        assert!(Document::parse(bytes).is_err());
    }
    let mut oversized = vec![b' '; MAX_BYTES + 1];
    oversized[..SOURCE.len()].copy_from_slice(SOURCE);
    assert!(Document::parse(&oversized).is_err());
}

#[test]
fn reviewed_intents_preserve_exact_source_text_and_refuse_unscoped_fields() -> Result<(), String> {
    use serde_json::json;
    let args = json!({"artifactId":"it_pdf_42","version":3,"sha256":"b".repeat(64),
        "action":{"op":"replace_text","page":0,"block":2,"expected":"Source paragraph.","text":"Reviewed paragraph."}});
    let (version, hash, action) = agent_tools::command("pdfcraft_edit", &args)?;
    assert_eq!(version, Some(3));
    assert_eq!(hash.as_deref(), Some("b".repeat(64).as_str()));
    assert_eq!(action, Some(args["action"].clone()));
    for (path, value) in [
        ("/version", json!(0)),
        ("/sha256", json!("B".repeat(64))),
        ("/action/page", json!(32)),
        ("/action/op", json!("file.open")),
        ("/action/text", json!("x\u{0}y")),
    ] {
        let mut bad = args.clone();
        *bad.pointer_mut(path).ok_or("missing fixture field")? = value;
        assert!(agent_tools::command("pdfcraft_edit", &bad).is_err());
    }
    let mut bad = args;
    bad["action"]["actorId"] = json!("forged");
    assert!(agent_tools::command("pdfcraft_edit", &bad).is_err());
    assert!(agent_tools::command(
        "read_pdfcraft",
        &json!({"artifactId":"it_pdf_42","page":31,"offset":16})
    )
    .is_ok());
    assert!(agent_tools::command(
        "read_pdfcraft",
        &json!({"artifactId":"it_pdf_42","page":null})
    )
    .is_err());
    assert!(agent_tools::command(
        "read_pdfcraft",
        &json!({"artifactId":"it_pdf_42","path":"/private"})
    )
    .is_err());
    Ok(())
}
