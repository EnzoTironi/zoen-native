use super::*;

#[test]
fn hidden_content_leaves_search_previews_quotes_mentions_and_message_actions() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "Secretphrase @Author")?;
    let quote = f.client.send_reply(
        f.input.space_id.clone(),
        "Visible answer".into(),
        theirs.id.clone(),
        false,
    )?;
    assert!(!f
        .client
        .lock()
        .search_messages(&f.input.space_id, "Secretphrase", None, 20)?
        .hits
        .is_empty());
    hide(&f, std::slice::from_ref(&theirs))?;
    let rows = timeline(&f)?;
    assert!(rows
        .iter()
        .find(|row| row.id == quote.id)
        .is_some_and(|row| row.reply_to.is_none()));
    assert!(f
        .client
        .lock()
        .search_messages(&f.input.space_id, "Secretphrase", None, 20)?
        .hits
        .is_empty());
    assert!(f
        .client
        .lock()
        .universal_search("Secretphrase", &["message".into()], 20)?
        .hits
        .is_empty());
    assert!(f.client.lock().search("Secretphrase").is_empty());
    assert!(!f
        .client
        .lock()
        .mentions()
        .iter()
        .any(|row| row.entry.id == theirs.id));
    assert!(!f
        .client
        .space(f.input.space_id.clone())?
        .last_preview
        .contains("Secretphrase"));
    assert!(f
        .client
        .lock()
        .reply_ref(&f.input.space_id, &theirs.id, false)
        .is_err());
    assert!(f
        .client
        .lock()
        .annotate(
            &f.input.space_id,
            &theirs.id,
            types::Annotation::React {
                emoji: "👍".into()
            }
        )
        .is_err());
    assert!(f
        .client
        .forward_message(
            f.input.space_id.clone(),
            theirs.id.clone(),
            f.input.space_id.clone()
        )
        .is_err());
    assert!(f
        .client
        .lock()
        .message_info(&crate::MessageInfoScope {
            expected_identity_id: f.input.expected_identity_id.clone(),
            expected_device_id: f.input.expected_device_id.clone(),
            space_id: f.input.space_id.clone(),
            message_id: theirs.id.clone(),
        })
        .is_err());
    Ok(())
}

#[test]
fn removing_a_deleted_placeholder_and_its_thread_reply_keeps_unselected_content() -> TestResult {
    let f = fixture(":memory:")?;
    let mine = own(&f)?;
    f.client.delete_messages(f.input.clone())?;
    let rows = timeline(&f)?;
    let deleted = rows
        .iter()
        .find(|row| row.id == mine.id)
        .ok_or("placeholder")?;
    assert!(deleted.deleted && deleted.text_revision.is_none());
    let mut input = f.input.clone();
    input.for_me = Some(true);
    input.targets = f
        .input
        .targets
        .iter()
        .map(|target| MessageDeletionTarget {
            message_id: target.message_id.clone(),
            expected_revision: target.message_id.clone(),
        })
        .collect();
    f.client.delete_messages(input)?;
    let rows = timeline(&f)?;
    assert!(!rows.iter().any(|row| row.id == mine.id));
    assert!(f.client.thread(f.input.space_id.clone(), mine.id).is_err());
    assert!(rows.iter().any(|row| row.id == f.unrelated.id));
    Ok(())
}

#[test]
fn corrupted_private_log_fails_closed_instead_of_resurrecting_removed_text() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "No resurrection")?;
    hide(&f, &[theirs])?;
    let personal = f.client.lock().personal_space().ok_or("personal")?;
    {
        let mut engine = f.client.lock();
        let mut event = engine
            .store
            .events(&personal)?
            .pop()
            .ok_or("private event")?;
        event.at_ms += 1; // Valid schema, invalid signature and hash.
        engine.store.conn().execute(
            "UPDATE events SET json = ?1 WHERE space = ?2 AND json LIKE '%MessagesDeletedForSelf%'",
            rusqlite::params![serde_json::to_string(&event)?, personal],
        )?;
        engine.reload()?;
    }
    assert!(timeline(&f).is_err());
    assert!(f
        .client
        .lock()
        .search_messages(&f.input.space_id, "resurrection", None, 20)
        .is_err());
    assert!(f
        .client
        .lock()
        .universal_search("resurrection", &[], 20)
        .is_err());
    assert!(f.client.delete_messages(f.input.clone()).is_err());
    Ok(())
}

#[test]
fn private_history_event_bounds_are_enforced_before_any_write() -> TestResult {
    let f = fixture(":memory:")?;
    let mut engine = f.client.lock();
    let personal = engine.ensure_personal_space()?;
    let before = engine.store.event_count()?;
    for messages in [
        vec![],
        vec!["duplicate".into(), "duplicate".into()],
        vec!["invalid\n".into()],
        (0..101).map(|index| format!("message-{index}")).collect(),
    ] {
        assert!(engine
            .append(
                &personal,
                &f.input.expected_identity_id,
                types::EventBody::MessagesDeletedForSelf {
                    space: f.input.space_id.clone(),
                    messages,
                }
            )
            .is_err());
    }
    assert_eq!(engine.store.event_count()?, before);
    Ok(())
}
