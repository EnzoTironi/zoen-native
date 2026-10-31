//! Option changes are private reviewed intents, never publication or acceptance.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn explicit_options_change_fences_old_send_and_preserves_plain_text_and_voice_transcription(
) -> TestResult {
    for kind in ["text", "file", "voice"] {
        let f = fixture(":memory:");
        let mut scope = f.scope.clone();
        if kind == "file" {
            scope.message_id = f
                .client
                .lock()
                .send_file_message_with_caption(
                    &scope.source_space,
                    "report.pdf",
                    "application/pdf",
                    b"opaque bounded bytes",
                    None,
                    None,
                    false,
                    "file-options",
                    "Private caption 雨",
                )?
                .id;
        }
        if kind == "voice" {
            scope.message_id = f
                .client
                .lock()
                .send_voice_note(
                    &scope.source_space,
                    b"opaque bounded audio",
                    "audio/mp4",
                    2400,
                    &[0.3, 0.8],
                    "Keep voice transcription",
                    None,
                    false,
                )?
                .id;
        }
        let review = f.client.forward_review(scope.clone())?;
        let before = counts(&f);
        let command = serde_json::json!({"op":"set_forward_options","scope":scope,"operation_id":review.operation_id,"source_revision":review.source_revision,"options":{"hide_author":true,"hide_caption":true}});
        let changed: serde_json::Value =
            serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
        assert_eq!(changed["options"]["hide_author"], true);
        assert_eq!(changed["options"]["hide_caption"], true);
        assert_eq!(counts(&f), before);
        assert!(f
            .client
            .confirm_forward(scope.clone(), review.operation_id, review.source_revision)
            .is_err());
        let accepted = f
            .client
            .confirm_forward(
                scope.clone(),
                changed["operation_id"]
                    .as_str()
                    .ok_or("new operation")?
                    .into(),
                changed["source_revision"].as_str().ok_or("source")?.into(),
            )?
            .accepted
            .ok_or("accepted")?;
        assert!(!accepted.forwarded);
        assert!(accepted.forwarded_author.is_none());
        match kind {
            "voice" => assert_eq!(
                accepted.voice.ok_or("voice")?.transcript,
                "Keep voice transcription"
            ),
            "file" => assert!(
                matches!(accepted.kind, crate::EntryKind::Message { text, .. } if text.is_empty())
            ),
            _ => assert!(
                matches!(accepted.kind, crate::EntryKind::Message { text, .. } if text == "Private forward 雨")
            ),
        }
    }
    Ok(())
}

#[test]
fn anonymous_same_room_voice_copies_the_item_without_normalizing_away_the_original() -> TestResult {
    let f = fixture(":memory:");
    let source = f.client.lock().send_voice_note(
        &f.scope.source_space,
        b"opaque bounded audio",
        "audio/mp4",
        2400,
        &[0.2, 0.8],
        "Retain transcription",
        None,
        false,
    )?;
    let copied = f.client.lock().forward_with_options(
        &f.scope.source_space,
        &source.id,
        &f.scope.source_space,
        ForwardOptions {
            hide_author: true,
            hide_caption: true,
        },
    )?;
    let rows = f.client.timeline(f.scope.source_space.clone())?;
    assert!(rows.iter().any(|entry| entry.id == source.id));
    assert!(rows.iter().any(|entry| entry.id == copied.id));
    assert_eq!(rows.iter().filter(|entry| entry.voice.is_some()).count(), 2);
    let original = source.voice.ok_or("source voice")?;
    let voice = copied.voice.ok_or("copy voice")?;
    assert_ne!(voice.item_id, original.item_id);
    assert_eq!(voice.transcript, original.transcript);
    assert_eq!(voice.levels, original.levels);
    assert_eq!(voice.ms, original.ms);
    assert!(!copied.forwarded);
    assert!(copied.forwarded_author.is_none());
    Ok(())
}

#[test]
fn rejected_options_preserve_the_exact_review_and_never_accept_changed_authority_or_source(
) -> TestResult {
    let f = fixture(":memory:");
    let review = f.client.forward_review(f.scope.clone())?;
    let options = ForwardOptions {
        hide_author: true,
        hide_caption: true,
    };
    let before = counts(&f);
    for part in ["owner", "device", "target"] {
        let mut scope = f.scope.clone();
        match part {
            "owner" => scope.expected_identity_id = "foreign-owner".into(),
            "device" => scope.expected_device_id = "foreign-device".into(),
            _ => scope.target_space = scope.source_space.clone(),
        }
        assert!(f
            .client
            .lock()
            .set_forward_options(
                &scope,
                &review.operation_id,
                &review.source_revision,
                options
            )
            .is_err());
        assert_eq!(
            f.client.forward_status(f.scope.source())?,
            Some(review.clone())
        );
        assert_eq!(counts(&f), before);
    }
    f.client.edit_message(
        f.scope.source_space.clone(),
        f.scope.message_id.clone(),
        "Changed since review".into(),
    )?;
    let after_edit = counts(&f);
    assert!(f
        .client
        .lock()
        .set_forward_options(
            &f.scope,
            &review.operation_id,
            &review.source_revision,
            options
        )
        .is_err());
    assert_eq!(f.client.forward_status(f.scope.source())?, Some(review));
    assert_eq!(counts(&f), after_edit);
    Ok(())
}

#[test]
fn private_options_survive_a_lost_response_and_cold_reopen_without_sending() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("forward-options")?);
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join("account.sqlite").to_string_lossy().into_owned();
    let f = fixture(&path);
    let review = f.client.forward_review(f.scope.clone())?;
    let before = counts(&f);
    let configured = f.client.lock().set_forward_options(
        &f.scope,
        &review.operation_id,
        &review.source_revision,
        ForwardOptions {
            hide_author: true,
            hide_caption: false,
        },
    )?;
    assert_eq!(counts(&f), before);
    assert_ne!(configured.operation_id, review.operation_id);
    assert_eq!(
        f.client.lock().set_forward_options(
            &f.scope,
            &configured.operation_id,
            &configured.source_revision,
            configured.options
        )?,
        configured
    );
    assert!(f
        .client
        .lock()
        .set_forward_options(
            &f.scope,
            &review.operation_id,
            &review.source_revision,
            ForwardOptions::default()
        )
        .is_err());
    let cipher: String = f.client.lock().store.conn().query_row(
        "SELECT value FROM meta WHERE key LIKE 'message.forward.%'",
        [],
        |r| r.get(0),
    )?;
    assert!(cipher.starts_with("v1:"));
    assert!(!cipher.contains("Private forward"));
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(
        client.forward_status(scope.source())?,
        Some(configured.clone())
    );
    assert_eq!(client.lock().store.outbox()?.len(), before.1);
    let first = client.confirm_forward(
        scope.clone(),
        configured.operation_id.clone(),
        configured.source_revision.clone(),
    )?;
    let accepted = first.accepted.ok_or("accepted")?;
    assert!(!accepted.forwarded);
    assert!(accepted.forwarded_author.is_none());
    let after = client.lock().store.outbox()?.len();
    assert_eq!(after, before.1 + 1);
    assert_eq!(
        client
            .confirm_forward(
                scope.clone(),
                configured.operation_id.clone(),
                configured.source_revision.clone()
            )?
            .accepted_message_id,
        Some(accepted.id)
    );
    assert!(client
        .lock()
        .set_forward_options(
            &scope,
            &configured.operation_id,
            &configured.source_revision,
            ForwardOptions::default()
        )
        .is_err());
    assert_eq!(client.lock().store.outbox()?.len(), after);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
