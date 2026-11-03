use super::*;
mod projections;

fn received(f: &Fixture, text: &str) -> TestResult<TimelineEntry> {
    let id = ordered(
        f,
        types::EventBody::MessagePosted {
            message: types::new_id("msg")?,
            text: text.into(),
            attaches: None,
            media: None,
            reply: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    f.client
        .timeline(f.input.space_id.clone())?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| "received message".into())
}
fn hide(f: &Fixture, rows: &[TimelineEntry]) -> TestResult<Vec<String>> {
    let mut input = f.input.clone();
    input.for_me = Some(true);
    input.targets = rows.iter().map(target).collect::<Result<_, _>>()?;
    Ok(f.client.delete_messages(input)?)
}
fn own(f: &Fixture) -> TestResult<TimelineEntry> {
    timeline(f)?
        .into_iter()
        .find(|row| row.id == f.input.targets[0].message_id)
        .ok_or_else(|| "own message".into())
}

#[test]
fn mixed_selection_is_private_atomic_and_does_not_change_source_or_outbox() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "Private removal unique received phrase")?;
    let mine = own(&f)?;
    let source = f.client.lock().state.spaces[&f.input.space_id]
        .entries
        .clone();
    let outbox = f.client.lock().store.outbox()?;
    let before = counts(&f.client)?;
    let ids = hide(&f, &[mine.clone(), theirs.clone()])?;
    assert_eq!(counts(&f.client)?, (before.0 + 1, before.1));
    assert_eq!(
        f.client
            .lock()
            .store
            .outbox()?
            .iter()
            .map(|row| &row.event.client_id)
            .collect::<Vec<_>>(),
        outbox
            .iter()
            .map(|row| &row.event.client_id)
            .collect::<Vec<_>>()
    );
    let engine = f.client.lock();
    assert_eq!(
        engine.state.spaces[&f.input.space_id].entries.len(),
        source.len()
    );
    assert!(engine.state.spaces[&f.input.space_id].entries.iter().all(|entry|
        !matches!(&entry.body, crate::engine::EntryBody::Message { marks, .. } if marks.deleted)));
    let personal = engine.personal_space().ok_or("personal")?;
    assert!(engine.logs[&personal].events().iter().any(|event|
        event.author == f.input.expected_identity_id && matches!(&event.body,
            types::EventBody::MessagesDeletedForSelf { space, messages } if space == &f.input.space_id && messages == &ids)));
    drop(engine);
    // The parent is redacted while its unselected thread reply remains available.
    let rows = timeline(&f)?;
    let anchor = rows
        .iter()
        .find(|row| row.id == mine.id)
        .ok_or("thread anchor")?;
    assert!(
        anchor.deleted
            && anchor.reply_to.is_none()
            && anchor.voice.is_none()
            && anchor.media.is_none()
    );
    assert!(
        matches!(&anchor.kind, crate::EntryKind::Message { text, card } if text.is_empty() && card.is_none())
    );
    assert!(!rows.iter().any(|row| row.id == theirs.id));
    assert!(rows.iter().any(|row| row.id == f.unrelated.id));
    assert_eq!(
        f.client
            .thread(f.input.space_id.clone(), mine.id.clone())?
            .len(),
        2
    );
    let after = counts(&f.client)?;
    hide(&f, &[mine, theirs])?;
    assert_eq!(counts(&f.client)?, after);
    Ok(())
}

#[test]
fn a_reader_can_remove_received_messages_without_fabricating_a_read_receipt() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "Received while unread")?;
    ordered(
        &f,
        types::EventBody::MemberAdded {
            identity: f.input.expected_identity_id.clone(),
            role: types::Role::Reader,
        },
    )?;
    let old_read = f
        .client
        .lock()
        .store
        .meta(&format!("read:{}", f.input.space_id))?;
    let before = f.client.space(f.input.space_id.clone())?;
    assert!(before.unread > 0);
    hide(&f, &[theirs])?;
    assert_eq!(
        f.client.space(f.input.space_id.clone())?.unread,
        before.unread - 1
    );
    assert_eq!(
        f.client
            .lock()
            .store
            .meta(&format!("read:{}", f.input.space_id))?,
        old_read
    );
    assert_eq!(f.client.lock().store.outbox()?.len(), 3);
    Ok(())
}

#[test]
fn failed_private_append_rolls_back_the_complete_view_and_all_storage() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "Keep after storage fault")?;
    let before = counts(&f.client)?;
    let rows = timeline(&f)?;
    f.client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER fail_private BEFORE INSERT ON events WHEN NEW.json LIKE '%MessagesDeletedForSelf%' BEGIN SELECT RAISE(ABORT,'injected private deletion failure'); END;")?;
    assert!(hide(&f, std::slice::from_ref(&theirs)).is_err());
    assert_eq!(counts(&f.client)?, before);
    assert_eq!(timeline(&f)?, rows);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_private;")?;
    hide(&f, &[theirs])?;
    Ok(())
}

#[test]
fn changed_last_target_refuses_every_private_removal() -> TestResult {
    let f = fixture(":memory:")?;
    let theirs = received(&f, "Unchanged received")?;
    let mine = own(&f)?;
    f.client.edit_message(
        f.input.space_id.clone(),
        mine.id.clone(),
        "Changed after review".into(),
    )?;
    let before = counts(&f.client)?;
    assert!(hide(&f, &[theirs.clone(), mine]).is_err());
    assert_eq!(counts(&f.client)?, before);
    assert!(timeline(&f)?.iter().any(|row| row.id == theirs.id));
    Ok(())
}

#[test]
fn private_removal_survives_restart_and_retry_without_an_extra_event() -> TestResult {
    let directory = std::env::temp_dir().join(types::new_id("private-deletion")?);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("client.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name)?;
    let theirs = received(&f, "Hidden after cold recovery")?;
    let mut input = f.input.clone();
    input.for_me = Some(true);
    input.targets = vec![target(&theirs)?];
    f.client.delete_messages(input.clone())?;
    let before = counts(&f.client)?;
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(name.into(), "en".into())?;
    assert!(client.unlock(vault)?);
    assert!(!client
        .timeline(input.space_id.clone())?
        .iter()
        .any(|row| row.id == theirs.id));
    assert_eq!(client.delete_messages(input)?, vec![theirs.id]);
    assert_eq!(counts(&client)?, before);
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn another_identity_and_a_shared_log_cannot_hide_this_accounts_messages() -> TestResult {
    let f = fixture(":memory:")?;
    let mine = own(&f)?;
    let body = types::EventBody::MessagesDeletedForSelf {
        space: f.input.space_id.clone(),
        messages: vec![mine.id.clone()],
    };
    let before = counts(&f.client)?;
    assert!(f
        .client
        .lock()
        .append(
            &f.input.space_id,
            &f.input.expected_identity_id,
            body.clone()
        )
        .is_err());
    assert_eq!(counts(&f.client)?, before);
    let foreign_personal = types::new_id("sp")?;
    f.client.lock().append(
        &foreign_personal,
        &f.founder,
        types::EventBody::SpaceCreated {
            title: "Foreign private log".into(),
            kind: types::SpaceKind::Personal,
            privacy: types::Privacy::EndToEnd,
        },
    )?;
    f.client
        .lock()
        .append(&foreign_personal, &f.founder, body)?;
    assert!(timeline(&f)?
        .iter()
        .any(|row| row.id == mine.id && !row.deleted));
    Ok(())
}
