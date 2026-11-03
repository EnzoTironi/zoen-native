//! Conversation custody wraps the existing builtin. No second voting state exists.
use super::*;
use crate::{QuoteSpan, ReplySelection};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn draft() -> serde_json::Value {
    serde_json::json!({"kind":"poll","reply_to":null,"question":"  Private poll 雨 😀  ","options":[{"id":"private_option_A","text":" First "},{"id":"private_option_B","text":"Second"}]})
}
fn public(engine: &Client) -> (Vec<i64>, usize, usize) {
    let counts = counts(engine);
    let e = engine.lock();
    (counts, e.state.items.len(), e.state.grants.len())
}
fn save_poll(f: &Fixture, revision: &str, value: &serde_json::Value) -> TestResult<MediaDraftDto> {
    Ok(f.engine
        .save_media_draft(f.scope.clone(), revision.into(), value.to_string(), vec![])?
        .draft)
}
fn item(entry: &TimelineEntry) -> TestResult<String> {
    match &entry.kind {
        crate::EntryKind::Message {
            card: Some(card), ..
        } => Ok(card.item_id.clone()),
        _ => Err("poll card missing".into()),
    }
}

#[test]
fn poll_custody_preserves_plain_selected_shared_and_thread_replies_and_publishes_one_builtin(
) -> TestResult {
    for mode in ["plain", "local", "selected", "shared", "thread"] {
        let mut f = fixture(":memory:");
        let source_space = if mode == "shared" {
            f.engine
                .create_group_with("Private source".into(), vec![], PrivacyDto::Closed)?
        } else {
            f.scope.space_id.clone()
        };
        let source = f
            .engine
            .send_message(source_space.clone(), "Alpha 😀 Beta".into())?;
        if mode == "thread" {
            f.scope.thread_root_id = Some(source.id.clone());
        }
        let mut value = draft();
        let selected = ReplySelection {
            source_id: source.id.clone(),
            revision: source.text_revision.clone().ok_or("revision")?,
            text: "😀".into(),
            spans: vec![QuoteSpan {
                start_utf16: 6,
                end_utf16: 8,
            }],
        };
        if mode == "shared" {
            let quote = f.engine.lock().review_shared_reply(
                &f.scope.normal(),
                &source_space,
                &source.id,
                Some(&selected),
            )?;
            value["shared_reply"] = serde_json::to_value(quote)?;
        } else if matches!(mode, "local" | "selected") {
            value["reply_to"] = source.id.clone().into();
        }
        if mode == "selected" {
            value["reply_quote"] = serde_json::to_value(selected)?;
        }
        if mode != "plain" && mode != "thread" {
            value["reply_intent"] = "private_poll_reply_A".into();
        }
        let before = public(&f.engine);
        let saved = save_poll(&f, "0", &value)?;
        assert!(saved.assets.is_empty());
        assert_eq!(public(&f.engine), before, "staging is private");
        assert!(rows(&f)
            .iter()
            .all(|(_, raw)| !raw.contains("Private poll")));
        let accepted = f
            .engine
            .send_media_draft(f.scope.clone(), saved.revision.clone())?;
        assert_eq!(accepted.entries.len(), 1);
        let entry = &accepted.entries[0];
        let reference = entry.reply_to.as_ref();
        if mode == "plain" {
            assert!(reference.is_none());
        } else if mode == "thread" {
            assert_eq!(entry.in_thread.as_deref(), Some(source.id.as_str()));
        } else {
            let reference = reference.ok_or("signed reply")?;
            assert_eq!(reference.id, source.id);
            if matches!(mode, "selected" | "shared") {
                assert_eq!(reference.text, "😀");
            }
            assert_eq!(
                reference
                    .origin
                    .as_ref()
                    .map(|origin| origin.space_id.as_str()),
                (mode == "shared").then_some(source_space.as_str())
            );
        }
        let detail = f.engine.item(item(entry)?)?;
        let app = detail.app.ok_or("canonical builtin state")?;
        assert_eq!(app.app_id, "poll");
        let view: serde_json::Value = serde_json::from_str(&app.view_json)?;
        assert_eq!(view["question"], "Private poll 雨 😀");
        assert_eq!(view["options"][0]["text"], "First");
        assert_eq!(view["options"][0]["votes"], serde_json::json!([]));
        let after = public(&f.engine);
        assert_eq!(after.1, before.1 + 1);
        assert_eq!(after.2, before.2 + 1);
        assert_eq!(
            f.engine
                .send_media_draft(f.scope.clone(), saved.revision)?
                .entries,
            accepted.entries
        );
        assert_eq!(
            public(&f.engine),
            after,
            "retry must not duplicate state/grants/messages"
        );
        let signed = f
            .engine
            .lock()
            .store
            .outbox_get(&entry.id)?
            .ok_or("signed message")?;
        let wire = serde_json::to_string(&signed)?;
        assert!(!wire.contains("private_poll_reply_A"));
        assert!(!wire.contains("private_option_"));
    }
    Ok(())
}

#[test]
fn incomplete_poll_work_is_private_editable_and_cannot_send_or_borrow_assets() -> TestResult {
    let f = fixture(":memory:");
    let before = public(&f.engine);
    let mut value = draft();
    value["question"] = "".into();
    value["options"] = serde_json::json!([{"id":"a","text":""},{"id":"b","text":""}]);
    let saved = save_poll(&f, "0", &value)?;
    assert!(matches!(
        f.engine
            .send_media_draft(f.scope.clone(), saved.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(f.engine.media_draft(f.scope.clone())?, saved);
    assert_eq!(public(&f.engine), before);
    for bad in [
        serde_json::json!({"options":[{"id":"a","text":"Only one"}]}),
        serde_json::json!({"question":"a".repeat(301)}),
        serde_json::json!({"options":[{"id":"a","text":"a"},{"id":"b","text":"b".repeat(201)}]}),
        serde_json::json!({"question":"bad\u{0000}input"}),
        serde_json::json!({"state":{"votes":["forged"]}}),
        serde_json::json!({"options":[{"id":"same","text":"a"},{"id":"same","text":"b"}]}),
        serde_json::json!({"options":[{"id":"a","text":"a","votes":["fake"]},{"id":"b","text":"b"}]}),
    ] {
        let mut candidate = draft();
        for (key, v) in bad.as_object().ok_or("object")? {
            candidate[key] = v.clone();
        }
        assert!(save_poll(&f, &saved.revision, &candidate).is_err());
        assert_eq!(f.engine.media_draft(f.scope.clone())?, saved);
    }
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            saved.revision.clone(),
            draft().to_string(),
            vec![input("a")]
        )
        .is_err());
    for options in [serde_json::json!(["a", "a"]), serde_json::json!(["a", " "])] {
        let mut candidate = draft();
        candidate["options"] = serde_json::json!([
            {"id":"a","text":options[0]}, {"id":"b","text":options[1]}
        ]);
        let current = f.engine.media_draft(f.scope.clone())?;
        let changed = save_poll(&f, &current.revision, &candidate)?;
        assert!(matches!(
            f.engine.send_media_draft(f.scope.clone(), changed.revision),
            Err(CoreError::DraftRejected { .. })
        ));
    }
    assert_eq!(public(&f.engine), before);
    Ok(())
}

#[test]
fn poll_message_failure_rolls_back_builtin_grant_document_and_receipt_before_exact_retry(
) -> TestResult {
    let f = fixture(":memory:");
    let saved = save_poll(&f, "0", &draft())?;
    let before = public(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_poll_message BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' BEGIN SELECT RAISE(ABORT,'synthetic poll failure'); END;")?;
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), saved.revision.clone())
        .is_err());
    assert_eq!(public(&f.engine), before);
    assert!(f.engine.media_draft(f.scope.clone())?.attempted);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_poll_message")?;
    let accepted = f
        .engine
        .send_media_draft(f.scope.clone(), saved.revision.clone())?;
    assert_eq!(accepted.entries.len(), 1);
    assert_eq!(public(&f.engine).1, before.1 + 1);
    let after = public(&f.engine);
    f.engine.send_media_draft(f.scope.clone(), saved.revision)?;
    assert_eq!(public(&f.engine), after);
    Ok(())
}

#[test]
fn poll_cold_receipt_recovers_hidden_output_without_installing_again_or_replacing_new_work(
) -> TestResult {
    let dir = temp()?;
    let path = dir.join("poll.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let saved = save_poll(&f, "0", &draft())?;
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(name.into(), "en".into())?;
    assert!(e.unlock(vault.clone())?);
    assert_eq!(e.media_draft(scope.clone())?, saved);
    let sent = e.send_media_draft(scope.clone(), saved.revision.clone())?;
    let entry = &sent.entries[0];
    e.delete_messages(crate::MessageDeletion {
        expected_identity_id: scope.expected_identity_id.clone(),
        expected_device_id: scope.expected_device_id.clone(),
        space_id: scope.space_id.clone(),
        for_me: Some(true),
        targets: vec![crate::MessageDeletionTarget {
            message_id: entry.id.clone(),
            expected_revision: entry
                .text_revision
                .clone()
                .unwrap_or_else(|| entry.id.clone()),
        }],
    })?;
    let mut next = draft();
    next["question"] = "New private question".into();
    let newer = e
        .save_media_draft(scope.clone(), sent.draft.revision, next.to_string(), vec![])?
        .draft;
    let before = public(&e);
    drop(e);
    let e = Client::open(name.into(), "en".into())?;
    assert!(e.unlock(vault)?);
    let replay = e.send_media_draft(scope.clone(), saved.revision.clone())?;
    assert!(replay.entries.is_empty());
    assert_eq!(replay.draft, newer);
    assert_eq!(public(&e), before);
    let receipt: serde_json::Value = serde_json::from_str(
        replay
            .draft
            .accepted_intent_json
            .as_deref()
            .ok_or("receipt")?,
    )?;
    assert_eq!(receipt["input_revision"], saved.revision);
    assert_eq!(receipt["messages"][0], entry.id);
    drop(e);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn poll_send_revalidates_selected_source_and_writer_without_erasing_private_edits() -> TestResult {
    for changed in ["edited", "deleted", "reader"] {
        let f = fixture(":memory:");
        let source = f
            .engine
            .send_message(f.scope.space_id.clone(), "Alpha 😀 Beta".into())?;
        let mut value = draft();
        value["reply_to"] = source.id.clone().into();
        value["reply_quote"] = serde_json::to_value(ReplySelection {
            source_id: source.id.clone(),
            revision: source.text_revision.ok_or("revision")?,
            text: "😀".into(),
            spans: vec![QuoteSpan {
                start_utf16: 6,
                end_utf16: 8,
            }],
        })?;
        value["reply_intent"] = "A".into();
        let saved = save_poll(&f, "0", &value)?;
        match changed {
            "edited" => {
                f.engine.edit_message(
                    f.scope.space_id.clone(),
                    source.id,
                    "Updated source".into(),
                )?;
            }
            "deleted" => {
                f.engine
                    .delete_message(f.scope.space_id.clone(), source.id)?;
            }
            _ => {
                f.engine.lock().append(
                    &f.scope.space_id,
                    &f.scope.expected_identity_id,
                    types::EventBody::MemberAdded {
                        identity: f.scope.expected_identity_id.clone(),
                        role: types::Role::Reader,
                    },
                )?;
            }
        }
        let before = public(&f.engine);
        assert!(matches!(
            f.engine
                .send_media_draft(f.scope.clone(), saved.revision.clone()),
            Err(CoreError::DraftRejected { .. })
        ));
        assert_eq!(f.engine.media_draft(f.scope.clone())?, saved);
        value["question"] = "Keep my new private question".into();
        let edited = save_poll(&f, &saved.revision, &value)?;
        assert!(!edited.attempted);
        assert!(matches!(
            f.engine
                .send_media_draft(f.scope.clone(), edited.revision.clone()),
            Err(CoreError::DraftRejected { .. })
        ));
        assert_eq!(public(&f.engine), before);
        let mut wrong = f.scope.clone();
        wrong.expected_device_id.push('x');
        assert!(f.engine.media_draft(wrong.clone()).is_err());
        assert!(f.engine.send_media_draft(wrong, edited.revision).is_err());
        if changed != "reader" {
            value["reply_to"] = serde_json::Value::Null;
            for key in ["reply_quote", "reply_intent"] {
                value.as_object_mut().ok_or("object")?.remove(key);
            }
            let cleared = save_poll(&f, &f.engine.media_draft(f.scope.clone())?.revision, &value)?;
            assert_eq!(
                f.engine
                    .send_media_draft(f.scope.clone(), cleared.revision)?
                    .entries
                    .len(),
                1
            );
        }
    }
    Ok(())
}
