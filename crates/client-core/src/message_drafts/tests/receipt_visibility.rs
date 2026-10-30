//! Acceptance belongs to the sealed receipt, independently of the current view.
use super::*;
use crate::{MessageDeletion, MessageDeletionTarget};

#[test]
fn accepted_normal_and_edit_receipts_survive_private_source_removal_without_reviving_content(
) -> Result<(), Box<dyn std::error::Error>> {
    for editing in [false, true] {
        let dir = temp()?;
        let path = dir.join("receipt.sqlite").to_string_lossy().into_owned();
        let f = fixture(&path);
        let (revision, accepted) = commit(&f, editing)?;
        f.engine.delete_messages(MessageDeletion {
            expected_identity_id: f.scope.expected_identity_id.clone(),
            expected_device_id: f.scope.expected_device_id.clone(),
            space_id: f.scope.space_id.clone(),
            for_me: Some(true),
            targets: vec![MessageDeletionTarget {
                message_id: visible_acceptance(&accepted).id.clone(),
                expected_revision: accepted
                    .entry
                    .as_ref()
                    .expect("visible accepted source")
                    .text_revision
                    .clone()
                    .expect("accepted revision"),
            }],
        })?;
        assert!(!f
            .engine
            .timeline(f.scope.space_id.clone())?
            .iter()
            .any(|entry| entry.id == visible_acceptance(&accepted).id));
        let before = counts(&f.engine);
        let current = read(&f);
        let retry = if editing {
            f.engine.lock().send_message_edit_draft(&f.scope, &revision)
        } else {
            f.engine.lock().send_message_draft(&f.scope, &revision)
        };
        assert!(
            retry.is_ok(),
            "a known accepted receipt must remain acknowledgeable after private removal: {retry:?}"
        );
        let retried = retry?;
        assert_eq!(retried.message_id, accepted.message_id);
        assert!(
            retried.entry.is_none(),
            "receipt must not expose hidden content"
        );
        assert_eq!(read(&f), current);
        assert_eq!(counts(&f.engine), before);
        assert!(!f
            .engine
            .timeline(f.scope.space_id.clone())?
            .iter()
            .any(|entry| entry.id == visible_acceptance(&accepted).id));
        let vault = f.vault.clone();
        let scope = f.scope.clone();
        let id = accepted.message_id.clone();
        let stamp = accepted.text_revision.clone();
        drop(f);
        let client = Client::open(path, "en".into())?;
        assert!(client.unlock(vault)?);
        let cold = if editing {
            client.lock().send_message_edit_draft(&scope, &revision)?
        } else {
            client.lock().send_message_draft(&scope, &revision)?
        };
        assert_eq!(cold.message_id, id);
        assert_eq!(cold.text_revision, stamp);
        assert!(cold.entry.is_none());
        assert_eq!(cold.draft, current);
        assert_eq!(counts(&client), before);
        drop(client);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn commit(f: &Fixture, editing: bool) -> R<(String, MessageDraftSendResult)> {
    let saved = if editing {
        let source = f
            .engine
            .send_message(f.scope.space_id.clone(), "Original".into())?;
        f.engine.lock().save_message_draft_content(
            &f.scope,
            "0",
            DraftContent {
                text: "Independent normal draft".into(),
                reply_to: None,
                reply_quote: None,
                shared_reply: None,
                edit: Some(MessageDraftEdit {
                    message_id: source.id,
                    source_revision: source.text_revision.expect("signed revision"),
                    base_text: "Original".into(),
                    text: "Accepted correction".into(),
                    is_caption: false,
                }),
            },
        )?
    } else {
        save(f, "0", "Accepted normal message", None)
    };
    let accepted = if editing {
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &saved.revision)?
    } else {
        f.engine
            .lock()
            .send_message_draft(&f.scope, &saved.revision)?
    };
    Ok((saved.revision, accepted))
}

#[test]
fn a_later_observed_edit_never_replaces_the_accepted_revision_stamp() -> R<()> {
    for editing in [false, true] {
        let f = fixture(":memory:");
        let (revision, accepted) = commit(&f, editing)?;
        let stamp = accepted
            .text_revision
            .clone()
            .expect("accepted signed revision");
        f.engine.edit_message(
            f.scope.space_id.clone(),
            accepted.message_id.clone(),
            "Later observed edit".into(),
        )?;
        let before = counts(&f.engine);
        let retry = if editing {
            f.engine
                .lock()
                .send_message_edit_draft(&f.scope, &revision)?
        } else {
            f.engine.lock().send_message_draft(&f.scope, &revision)?
        };
        assert_eq!(retry.text_revision.as_deref(), Some(stamp.as_str()));
        assert_ne!(
            visible_acceptance(&retry).text_revision.as_deref(),
            Some(stamp.as_str())
        );
        assert!(
            matches!(&visible_acceptance(&retry).kind, crate::EntryKind::Message { text, .. } if text == "Later observed edit")
        );
        assert_eq!(counts(&f.engine), before);
        let current = read(&f);
        let continued = f.engine.lock().save_message_draft_content(
            &f.scope,
            &current.revision,
            DraftContent {
                text: current.text,
                reply_to: current.reply_to,
                reply_quote: current.reply_quote,
                shared_reply: current.shared_reply,
                edit: Some(MessageDraftEdit {
                    message_id: accepted.message_id,
                    source_revision: stamp,
                    base_text: if editing {
                        "Accepted correction".into()
                    } else {
                        "Accepted normal message".into()
                    },
                    text: "Continued typing".into(),
                    is_caption: false,
                }),
            },
        )?;
        assert!(matches!(
            f.engine
                .lock()
                .send_message_edit_draft(&f.scope, &continued.revision),
            Err(CoreError::DraftRejected { .. })
        ));
        assert_eq!(counts(&f.engine), before);
    }
    Ok(())
}

#[test]
fn an_old_receipt_without_a_stamp_acknowledges_without_borrowing_the_current_revision() -> R<()> {
    let f = fixture(":memory:");
    let (revision, accepted) = commit(&f, false)?;
    {
        let engine = f.engine.lock();
        let storage = engine.draft_storage_scope(&f.scope)?;
        let mut record = engine.load_message_draft(&f.scope, &storage)?;
        record
            .last_accepted
            .as_mut()
            .expect("accepted receipt")
            .text_revision = None;
        engine.persist_message_draft(&storage, &record)?;
    }
    let before = counts(&f.engine);
    let retry = f.engine.lock().send_message_draft(&f.scope, &revision)?;
    assert_eq!(retry.message_id, accepted.message_id);
    assert!(retry.text_revision.is_none());
    assert!(visible_acceptance(&retry).text_revision.is_some());
    assert_eq!(counts(&f.engine), before);
    Ok(())
}
