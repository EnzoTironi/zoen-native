use super::*;

#[test]
fn preview_format_tracks_complete_message_edits_deletion_and_legacy_dtos(
) -> Result<(), Box<dyn std::error::Error>> {
    let client = seeded();
    let space = find_space(&client, "Zoen").id;
    let body = format!(
        "> **{}**\n> [Read more](https://example.org/a_(b))",
        "café🦊 ".repeat(80)
    );
    let message = client.send_message(space.clone(), body.clone())?;
    let summary = client.space(space.clone())?;
    assert_eq!(
        summary.last_preview, body,
        "never parse a truncated Markdown source"
    );
    assert!(summary.last_preview_is_markdown);

    let mut legacy = serde_json::to_value(&summary)?;
    if let serde_json::Value::Object(fields) = &mut legacy {
        fields.remove("last_preview_is_markdown");
    }
    let decoded: SpaceSummary = serde_json::from_value(legacy)?;
    assert!(
        !decoded.last_preview_is_markdown,
        "old DTOs never guess label syntax"
    );
    assert_eq!(decoded.last_preview, body);

    client.edit_message(
        space.clone(),
        message.id.clone(),
        "`edited literal_*`".into(),
    )?;
    let edited = client.space(space.clone())?;
    assert_eq!(edited.last_preview, "`edited literal_*`");
    assert!(edited.last_preview_is_markdown);
    client.lock().reload()?;
    assert_eq!(
        client.space(space.clone())?,
        edited,
        "replay preserves format provenance"
    );

    client.delete_message(space.clone(), message.id)?;
    let deleted = client.space(space)?;
    assert!(!deleted.last_preview_is_markdown);
    assert_ne!(deleted.last_preview, edited.last_preview);
    Ok(())
}

#[test]
fn preview_keeps_attachment_titles_literal_but_renders_their_authored_captions(
) -> Result<(), CoreError> {
    let client = seeded();
    let space = find_space(&client, "Zoen").id;
    let filename = "**report_[1]**.pdf";
    let message = client.send_file_message_with_caption(
        space.clone(),
        filename.into(),
        "application/pdf".into(),
        vec![7; 64],
        None,
        None,
        false,
        "preview-file".into(),
        String::new(),
    )?;
    let summary = client.space(space.clone())?;
    assert!(summary.last_preview.contains(filename));
    assert!(!summary.last_preview_is_markdown);
    client.edit_message(
        space.clone(),
        message.id.clone(),
        "**caption** café🦊".into(),
    )?;
    let caption = client.space(space.clone())?;
    assert_eq!(caption.last_preview, "**caption** café🦊");
    assert!(caption.last_preview_is_markdown);
    client.edit_message(space.clone(), message.id, String::new())?;
    assert_eq!(client.space(space)?.last_preview, summary.last_preview);
    Ok(())
}
