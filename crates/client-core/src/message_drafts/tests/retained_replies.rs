//! New private typing must survive beside a previously reviewed, unavailable reply.
use super::*;
use crate::{MessageDeletion, MessageDeletionTarget, QuoteSpan, ReplySelection};

fn changed_text(draft: &MessageDraftDto, text: &str) -> DraftContent {
    DraftContent {
        reply_intent: None,
        text: text.into(),
        edit: draft.edit.clone(),
        reply_to: draft.reply_to.clone(),
        reply_quote: draft.reply_quote.clone(),
        shared_reply: draft.shared_reply.clone(),
    }
}

#[test]
fn existing_reply_context_preserves_new_text_but_still_cannot_publish_an_unavailable_source(
) -> Result<(), Box<dyn std::error::Error>> {
    for (kind, loss) in [
        ("plain", "deleted"),
        ("plain", "private"),
        ("selection", "deleted"),
        ("selection", "edited"),
        ("shared", "deleted"),
        ("shared", "edited"),
        ("shared", "access"),
    ] {
        let dir = temp()?;
        let path = dir.join("retained.sqlite").to_string_lossy().into_owned();
        let f = fixture(&path);
        let space = if kind == "shared" {
            f.engine
                .create_group_with("Source".into(), vec![], PrivacyDto::Closed)?
        } else {
            f.scope.space_id.clone()
        };
        let source = f
            .engine
            .send_message(space.clone(), "Original quotation".into())?;
        let selection = (kind == "selection").then(|| ReplySelection {
            source_id: source.id.clone(),
            revision: source.text_revision.clone().expect("signed source"),
            text: "Original".into(),
            spans: vec![QuoteSpan {
                start_utf16: 0,
                end_utf16: 8,
            }],
        });
        let shared = if kind == "shared" {
            Some(
                f.engine
                    .lock()
                    .review_shared_reply(&f.scope, &space, &source.id, None)?,
            )
        } else {
            None
        };
        let saved = f.engine.lock().save_message_draft_content(
            &f.scope,
            "0",
            DraftContent {
                reply_intent: None,
                text: "Before source change".into(),
                edit: None,
                reply_to: (kind != "shared").then(|| source.id.clone()),
                reply_quote: selection,
                shared_reply: shared,
            },
        )?;
        match loss {
            "deleted" => f.engine.delete_message(space.clone(), source.id.clone())?,
            "private" => {
                f.engine.delete_messages(MessageDeletion {
                    expected_identity_id: f.scope.expected_identity_id.clone(),
                    expected_device_id: f.scope.expected_device_id.clone(),
                    space_id: space.clone(),
                    for_me: Some(true),
                    targets: vec![MessageDeletionTarget {
                        message_id: source.id.clone(),
                        expected_revision: source.text_revision.clone().expect("reviewed revision"),
                    }],
                })?;
            }
            "edited" => {
                f.engine
                    .edit_message(space.clone(), source.id, "Later source text".into())?
            }
            "access" => {
                // Same membership projection as authenticated source removal;
                // destination membership/ownership is intentionally unchanged.
                f.engine
                    .lock()
                    .state
                    .spaces
                    .get_mut(&space)
                    .ok_or("source")?
                    .members
                    .clear();
            }
            _ => unreachable!("fixed cases"),
        }
        let before = counts(&f.engine);
        let latest = f.engine.lock().save_message_draft_content(
            &f.scope,
            &saved.revision,
            changed_text(&saved, "New private typing after source loss 😀"),
        );
        assert!(latest.is_ok(), "{kind}/{loss}: private typing must not depend on current source availability: {latest:?}");
        let latest = latest?;
        assert_eq!(latest.reply_to, saved.reply_to);
        assert_eq!(latest.reply_quote, saved.reply_quote);
        assert_eq!(latest.shared_reply, saved.shared_reply);
        assert_eq!(counts(&f.engine), before);
        assert!(
            matches!(
                f.engine
                    .lock()
                    .send_message_draft(&f.scope, &latest.revision),
                Err(CoreError::DraftRejected { .. })
            ),
            "{kind}/{loss}: publication still requires the live source"
        );
        assert_eq!(read(&f), latest);
        assert_eq!(counts(&f.engine), before);
        let mut substituted = changed_text(&latest, "Do not save this substitution");
        if let Some(quote) = &mut substituted.reply_quote {
            quote.text = "Forged excerpt".into();
        } else if let Some(quote) = &mut substituted.shared_reply {
            quote.author_name = "Forged author".into();
        } else {
            substituted.reply_to = Some("unavailable-other-source".into());
        }
        assert!(f
            .engine
            .lock()
            .save_message_draft_content(&f.scope, &latest.revision, substituted)
            .is_err());
        for device in [false, true] {
            let mut foreign = f.scope.clone();
            if device {
                foreign.expected_device_id = "another-device".into();
            } else {
                foreign.expected_identity_id = "another-account".into();
            }
            assert!(f
                .engine
                .lock()
                .save_message_draft_content(
                    &foreign,
                    &latest.revision,
                    changed_text(&latest, "Foreign control")
                )
                .is_err());
        }
        assert!(f
            .engine
            .lock()
            .save_message_draft_content(
                &f.scope,
                &saved.revision,
                changed_text(&latest, "Stale overwrite")
            )
            .is_err());
        assert_eq!(read(&f), latest);
        assert_eq!(counts(&f.engine), before);
        let vault = f.vault.clone();
        let scope = f.scope.clone();
        drop(f);
        let client = Client::open(path, "en".into())?;
        assert!(client.unlock(vault)?);
        assert_eq!(
            client.lock().message_draft(&scope)?,
            latest,
            "{kind}/{loss}: cold recovery"
        );
        let mut cancelled = changed_text(&latest, &latest.text);
        cancelled.reply_to = None;
        cancelled.reply_quote = None;
        cancelled.shared_reply = None;
        let cleared =
            client
                .lock()
                .save_message_draft_content(&scope, &latest.revision, cancelled)?;
        assert_eq!(cleared.text, latest.text);
        assert_eq!(
            counts(&client),
            before,
            "cancelling the reference does not send"
        );
        let accepted = client
            .lock()
            .send_message_draft(&scope, &cleared.revision)?;
        let visible = visible_acceptance(&accepted);
        assert!(visible.reply_to.is_none());
        assert!(
            matches!(&visible.kind, crate::EntryKind::Message { text, .. } if text == &latest.text)
        );
        assert_eq!(
            counts(&client),
            (before.0, before.1 + 1),
            "one explicit normal send"
        );
        drop(client);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
