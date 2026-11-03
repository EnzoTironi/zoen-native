//! Actual browser command against signed Page events and the existing draft journal.
use super::*;

#[test]
fn browser_export_selected_page_version_excludes_private_draft_and_survives_snapshot(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let original = "# Original 🪴\r\n\r\nRaw **text** e\u{301}\r\n";
    let made = doc(
        &mut client,
        &serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"original.md","markdown":original}),
        10,
    )?;
    let id = made.result["id"]
        .as_str()
        .ok_or("missing authenticated test as_str value")?
        .to_owned();
    confirm_documents(
        &mut client,
        &mut log,
        made.update.ok_or("missing authenticated test update")?,
        11,
    )?;
    let first = doc(
        &mut client,
        &serde_json::json!({"op":"page","item_id":id}),
        12,
    )?
    .result;
    let mut changed = first["blocks"][1].clone();
    changed["text"] = "PRIVATE WORDS".into();
    changed["spans"] = serde_json::json!([]);
    let applied = doc(
        &mut client,
        &serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"private","edit_context":first["edit_context"],"order":first["blocks"].as_array().ok_or("missing authenticated test as_array value")?.iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[changed]}),
        13,
    )?;
    let snapshot = applied
        .update
        .ok_or("missing authenticated test update")?
        .snapshot;
    let mut reopened = Client::new(client.account.clone(), client.signer.clone(), &snapshot)?;
    let before = serde_json::to_vec(&reopened.saved.documents)?;
    let command = serde_json::json!({"op":"page_export_version", "space_id":log.space(),"item_id":id,"version":1,"observed_hash":first["content_hash"]});
    let exported = doc(&mut reopened, &command, 14)?;
    assert_eq!(exported.result["markdown"], original);
    assert_eq!(exported.result["title"], first["title"]);
    assert_eq!(exported.result["content_hash"], first["content_hash"]);
    assert!(
        exported.update.is_none() && exported.downloads.is_empty() && exported.uploads.is_empty()
    );
    assert_eq!(serde_json::to_vec(&reopened.saved.documents)?, before);
    let draft = doc(
        &mut reopened,
        &serde_json::json!({"op":"page_markdown","item_id":id}),
        15,
    )?;
    assert!(draft
        .result
        .as_str()
        .ok_or("missing authenticated test as_str value")?
        .contains("PRIVATE WORDS"));
    for (key, value) in [
        ("version", serde_json::json!(0)),
        ("version", serde_json::json!(2)),
        ("observed_hash", serde_json::json!("stale")),
        ("space_id", serde_json::json!("foreign")),
    ] {
        let mut invalid = command.clone();
        invalid[key] = value;
        assert!(reopened.document_request(&invalid.to_string(), 16).is_err());
    }
    reopened
        .saved
        .integrity_errors
        .insert(log.space().into(), "injected integrity failure".into());
    assert!(reopened.document_request(&command.to_string(), 17).is_err());
    reopened.saved.integrity_errors.clear();
    let me = reopened.account.identity.clone();
    let frame = clear(
        &mut log,
        &Author::device(
            &NativeSigner::from_secret(&[71; 32]),
            NativeSigner::from_secret(&[72; 32]),
        ),
        EventBody::MemberRemoved { identity: me },
        18,
    )?;
    reopened.receive(&frame, 19)?;
    assert!(reopened.document_request(&command.to_string(), 20).is_err());
    assert_eq!(serde_json::to_vec(&reopened.saved.documents)?, before);
    Ok(())
}
