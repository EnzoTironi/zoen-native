//! Complete recovery authority scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn media_contract_recovery_only_file_refuses_legacy_attachment_command(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    media_contract_recovery_command_refused("legacy")?;

    Ok(())
}

pub(super) fn media_contract_recovery_only_file_refuses_new_version_before_writes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    media_contract_recovery_command_refused("version")?;

    Ok(())
}

pub(super) fn media_contract_recovery_only_file_refuses_revert_command(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    media_contract_recovery_command_refused("revert")?;

    Ok(())
}

pub(super) fn media_contract_recovery_only_file_refuses_signed_projection_and_ingest(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for operation in ["legacy", "version", "revert"] {
        let dir = temp()?;
        let path = dir.join("recovery-authority.sqlite");
        let (f, item) = media_contract_recovered_writer_at(path.to_str().unwrap())?;
        let content = f.engine.lock().state.items[&item].current().content.clone();
        let body = match operation {
            "legacy" => types::EventBody::MessagePosted {
                message: types::new_id("msg")?,
                text: "signed remote recovery claim".into(),
                attaches: Some(item.clone()),
                media: None,
                reply: None,
                forwarded: false,
                forwarded_author: None,
            },
            "version" => types::EventBody::ItemVersioned {
                item: item.clone(),
                content,
                note: "signed remote version".into(),
            },
            "revert" => types::EventBody::ItemReverted {
                item: item.clone(),
                to_version: 1,
                note: "signed remote revert".into(),
            },
            _ => unreachable!(),
        };
        {
            let mut engine = f.engine.lock();
            let event = engine
                .author_for(&f.scope.expected_identity_id)
                .unwrap()
                .sign_event(
                    &f.scope.space_id,
                    &types::new_id("remote")?,
                    crate::engine::now_ms(),
                    engine.logs[&f.scope.space_id].head(),
                    body.clone(),
                )?;
            event_log::verify_author(&event).unwrap();
            let context = crate::pages::PlanReplay::new(&engine.store, &engine.logs)
                .local(&engine.state, &engine.identities, &event)
                .unwrap();
            let identities = engine.identities.clone();
            assert_eq!(
                engine.state.apply_checked(&event, &identities, &context),
                crate::pages::Projection::Refused,
                "signed {operation} gained authority in the recovery overlay"
            );
        }
        media_contract_receive(&f, body, false)?;
        assert_eq!(f.engine.lock().state.items[&item].versions.len(), 1);
        assert!(!f
            .engine
            .timeline(f.scope.space_id.clone())
            .unwrap()
            .iter()
            .any(
                |entry| matches!(&entry.kind, crate::EntryKind::Message { text, .. }
                if text == "signed remote recovery claim")
            ));
        let vault = f.vault.clone();
        let scope = f.scope.clone();
        drop(f);
        let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
        assert!(reopened.unlock(vault).unwrap());
        assert_eq!(reopened.lock().state.items[&item].versions.len(), 1);
        assert!(!reopened.timeline(scope.space_id).unwrap().iter().any(
            |entry| matches!(&entry.kind, crate::EntryKind::Message { text, .. }
                if text == "signed remote recovery claim")
        ));
        assert_eq!(
            reopened.file_bytes(item, None).unwrap(),
            Some(input("recovery").bytes)
        );
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }
    Ok(())
}

pub(super) fn media_contract_recovery_bytes_can_be_explicitly_copied_to_a_fresh_file(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (f, item) = media_contract_recovered_writer()?;
    let bytes = f.engine.file_bytes(item.clone(), None).unwrap().unwrap();
    let copied = f
        .engine
        .lock()
        .file_add(
            &f.scope.space_id,
            "",
            "copied.pdf",
            "application/pdf",
            &bytes,
            None,
        )
        .unwrap();
    assert_ne!(copied, item);
    f.engine
        .lock()
        .post_as(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            "explicit fresh copy",
            Some(copied.clone()),
        )
        .unwrap();
    assert_eq!(f.engine.file_bytes(copied, None).unwrap(), Some(bytes));

    Ok(())
}

pub(super) fn media_contract_recovery_reservation_yields_to_a_validated_file_creation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (f, item) = media_contract_recovered_writer()?;
    let source = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| matches!(&row.event.body, types::EventBody::ItemCreated { item: id, .. } if id == &item))
        .unwrap()
        .event;
    {
        let mut engine = f.engine.lock();
        event_log::verify_author(&source).unwrap();
        let event = engine.logs[&f.scope.space_id]
            .clone()
            .sequence(source)?
            .clone();
        let _ = engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(&event)?,
        });
    }
    assert!(!f.engine.lock().state.is_recovery_item(&item));
    f.engine
        .lock()
        .file_new_version_typed(
            &item,
            b"authorized after actual creation",
            None,
            "real version",
            "confirmed.pdf",
            "application/pdf",
        )
        .unwrap();
    assert_eq!(
        f.engine.file_bytes(item, None).unwrap(),
        Some(b"authorized after actual creation".to_vec())
    );

    Ok(())
}
