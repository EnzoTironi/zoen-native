//! Reader denial at the public message entrypoints, before durable or projected writes.
use super::*;

fn snapshot(f: &Fixture) -> TestResult<serde_json::Value> {
    let e = f.client.lock();
    let outbox: Vec<_> = e
        .store
        .outbox()?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "event": row.event,
                "attempts": row.attempts,
                "failed": row.failed,
                "error": row.last_error,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "databaseChanges": e.store.conn().total_changes(),
        "events": e.store.event_count()?,
        "log": e.logs.get(&f.space).ok_or("missing confirmed log")?.events(),
        "outbox": outbox,
        "pending": e.net.pending,
        "timeline": e.timeline(&f.space)?,
        "summary": e.space_summary(&f.space)?,
        "roles": e.member_roles(&f.space)?,
    }))
}

fn confirm_message(f: &Fixture, client_id: &str) -> TestResult<()> {
    let pending = f
        .client
        .lock()
        .store
        .outbox()?
        .into_iter()
        .find(|row| row.event.client_id == client_id)
        .ok_or("missing genuine pending message")?
        .event;
    f.confirm(pending)
}

fn refuses<T>(
    f: &Fixture,
    before: &serde_json::Value,
    action: &str,
    result: Result<T, CoreError>,
) -> TestResult<()> {
    assert!(
        matches!(result, Err(CoreError::Forbidden { .. })),
        "{action} must refuse the Reader before signing or enqueue"
    );
    assert_eq!(
        snapshot(f)?,
        *before,
        "{action} must preserve the database, exact outbox and projection"
    );
    Ok(())
}

fn demoted_reader(kind: SpaceKind, encrypted: bool) -> TestResult<()> {
    let f = Fixture::new(kind)?;
    let original = f
        .client
        .send_message(f.space.clone(), "Message written before demotion".into())?;
    confirm_message(&f, &original.id)?;
    // The exact reply targets are valid and writable before the role change.
    for thread in [false, true] {
        let reply = f.client.send_reply(
            f.space.clone(),
            "Writer control".into(),
            original.id.clone(),
            thread,
        )?;
        confirm_message(&f, &reply.id)?;
    }
    assert!(f.client.space(f.space.clone())?.can_write);
    if encrypted {
        // This is the normal authenticated, canonical privacy transition. These
        // entrypoint negatives must refuse before any MLS sealing/group lookup;
        // this fixture does not claim a new encrypted transport qualification.
        f.ordered(EventBody::SpaceEncrypted)?;
    }
    f.ordered(EventBody::MemberAdded {
        identity: f.identity.clone(),
        role: Role::Reader,
    })?;
    let summary = f.client.space(f.space.clone())?;
    assert!(!summary.can_write);
    assert_eq!(
        summary.privacy,
        if encrypted {
            PrivacyDto::EndToEnd
        } else {
            PrivacyDto::Closed
        }
    );
    assert_eq!(
        f.client
            .member_roles(f.space.clone())?
            .into_iter()
            .find(|row| row.identity_id == f.identity)
            .ok_or("current reader missing")?
            .role,
        "reader"
    );
    let before = snapshot(&f)?;
    refuses(
        &f,
        &before,
        "send",
        f.client
            .send_message(f.space.clone(), "Reader must not post".into()),
    )?;
    for thread in [false, true] {
        refuses(
            &f,
            &before,
            if thread {
                "thread reply"
            } else {
                "inline reply"
            },
            f.client.send_reply(
                f.space.clone(),
                "Reader must not reply".into(),
                original.id.clone(),
                thread,
            ),
        )?;
    }
    refuses(
        &f,
        &before,
        "edit own earlier message",
        f.client.edit_message(
            f.space.clone(),
            original.id.clone(),
            "Reader must not replace earlier text".into(),
        ),
    )?;
    refuses(
        &f,
        &before,
        "delete own earlier message",
        f.client
            .delete_message(f.space.clone(), original.id.clone()),
    )?;
    Ok(())
}

#[test]
fn demoted_reader_cannot_send_reply_or_edit_closed_conversations() -> TestResult<()> {
    for kind in [SpaceKind::Group, SpaceKind::Community] {
        demoted_reader(kind, false)?;
    }
    Ok(())
}

#[test]
fn demoted_reader_cannot_send_reply_or_edit_after_canonical_encryption() -> TestResult<()> {
    demoted_reader(SpaceKind::Group, true)
}
