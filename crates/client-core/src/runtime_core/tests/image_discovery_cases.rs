//! Complete image discovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn bounded_discovery_cursor_survives_complete_image(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("discovery-owner-trust", Some(1000))?;
    for index in 0..17 {
        fixture.message(&format!("Retained request {index}"))?;
    }
    let first = fixture.core.discover_replies(SPACE, 400).unwrap();
    assert_eq!(first.replies.len(), 16);
    assert!(first.changed && !first.complete && first.examined <= 64);
    let first_triggers: Vec<_> = first
        .replies
        .iter()
        .map(|facts| facts.intent().trigger().to_string())
        .collect();
    let image = fixture.core.image().unwrap();
    let mut reopened = DeviceCore::restore(image, fixture.credential()).unwrap();
    let second = reopened.discover_replies(SPACE, 400).unwrap();
    assert_eq!(second.replies.len(), 1);
    assert!(second.changed && second.complete);
    assert!(!first_triggers.contains(&second.replies[0].intent().trigger().to_string()));
    let repeated = reopened.discover_replies(SPACE, 400).unwrap();
    assert!(repeated.replies.is_empty() && repeated.complete && !repeated.changed);
    assert_eq!(repeated.examined, 0);

    Ok(())
}

pub(super) fn discovery_refuses_cursor_outside_verified_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("discovery-owner-trust", Some(1000))?;
    fixture.message("Actual verified owner request")?;
    fixture
        .core
        .engine
        .store
        .set_meta(
            &format!("runtime.reply-scan/1/{SPACE}"),
            &serde_json::to_string(&Seen {
                seq: u64::MAX,
                hash: "fabricated".into(),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        fixture.core.discover_replies(SPACE, 400),
        Err(NativeError::History)
    ));

    Ok(())
}

pub(super) fn actual_owner_opening_survives_complete_image_and_pins_original_input(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("explicit-owner-trust", Some(1000))?;
    let trigger = fixture.message("Private retained owner request")?;
    let peer_event = fixture.core.engine.logs[SPACE].events().last().unwrap();
    assert_eq!(
        crate::mls_opening::opened_leaf(&fixture.core.engine.store, peer_event)
            .unwrap()
            .identity,
        fixture.owner_profile.id
    );
    let own_event = fixture.owner.logs[SPACE].events().last().unwrap();
    assert!(crate::mls_opening::opened_leaf(&fixture.owner.store, own_event).is_none());
    assert!(crate::mls_opening::own_echo(
        &fixture.owner.store,
        own_event,
        fixture.owner.account().unwrap()
    ));
    let live = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    assert_eq!(live.text(), "Private retained owner request\n");
    let original = live.intent().clone();
    assert_eq!(live.expires_at_ms(), Some(1000));
    let image = fixture.core.image().unwrap();
    assert!(image.as_bytes().len() < MAX_MEMORY_IMAGE_BYTES);
    let cold = DeviceCore::restore(image, fixture.credential()).unwrap();
    let restored = cold.refresh(&original, 400).unwrap();
    assert_eq!(restored.frontier(), live.frontier());
    assert_eq!(restored.text(), live.text());
    fixture.message("A later request cannot rewrite a held attempt")?;
    let refreshed = fixture.core.refresh(&original, 500).unwrap();
    assert_ne!(refreshed.frontier().hash(), live.frontier().hash());
    assert_eq!(refreshed.intent(), &original);
    assert_eq!(refreshed.text(), live.text());
    assert!(matches!(
        fixture.core.refresh(&original, 1000),
        Err(NativeError::Permission)
    ));
    println!("JOURNEY certified real Welcome/Commit/Application; live/cold equivalent; original input pinned; expiry denied");

    Ok(())
}

pub(super) fn whole_image_keeps_non_openmls_provider_tables_and_real_pending_outbox(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.core.engine.store.conn().execute_batch(
        "INSERT INTO vc_emulation_bindings VALUES (1,x'01',x'0203'); INSERT INTO registered_vc_emulation_epochs VALUES (1,x'01',x'0405');"
    ).unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    let pending = fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            400,
            EventBody::MessagePosted {
                message: new_id("message")?,
                text: "Retain protocol queue".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )
        .unwrap();
    let cold = DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
    for (table, column, expected) in [
        ("vc_emulation_bindings", "bindings", vec![2u8, 3]),
        (
            "registered_vc_emulation_epochs",
            "registration",
            vec![4u8, 5],
        ),
    ] {
        let bytes: Vec<u8> = cold
            .engine
            .store
            .conn()
            .query_row(
                &format!("SELECT {column} FROM {table} WHERE group_id=x'01'"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bytes, expected);
    }
    assert_eq!(
        cold.engine.store.outbox_get(&pending.client_id).unwrap(),
        Some(pending)
    );
    assert_eq!(
        cold.engine.device().unwrap().epoch(SPACE).unwrap(),
        fixture.core.engine.device().unwrap().epoch(SPACE).unwrap()
    );
    println!("JOURNEY complete image retains actual joined MLS group, pending signed outbox and non-openmls provider table bytes");

    Ok(())
}

pub(super) fn image_rejects_legacy_schema_provider_and_secrets_before_migrations(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    for statement in [
        "PRAGMA user_version=1",
        "UPDATE meta SET value='2' WHERE key='event_format'",
        "CREATE TABLE injected (secret BLOB)",
        "DELETE FROM openmls_sqlite_storage_migrations WHERE version=6",
        "UPDATE identities SET secret=zeroblob(32)",
        "INSERT INTO meta VALUES ('zoen.identity.v1','root-secret')",
        "INSERT INTO vc_emulation_group_secrets VALUES (99, x'01', 'pprf', x'02')",
    ] {
        let image = fixture.core.image().unwrap();
        let store = Store::from_memory_image(image.as_bytes()).unwrap();
        store.conn().execute_batch(statement).unwrap();
        let bytes = store.memory_image().unwrap();
        let before = store.event_count().unwrap();
        let rejected = DeviceCore::restore(
            NativeImage::decode_bounded(bytes).unwrap(),
            fixture.credential(),
        );
        assert!(matches!(rejected, Err(NativeError::Image)), "{statement}");
        assert_eq!(
            store.event_count().unwrap(),
            before,
            "refusal must precede destructive migration"
        );
    }
    assert!(NativeImage::decode_bounded(vec![0; MAX_MEMORY_IMAGE_BYTES + 1]).is_err());
    assert!(NativeImage::decode_bounded(b"SQLite format 3\0".to_vec()).is_err());
    assert!(!format!("{:?}", fixture.core.image().unwrap()).contains("SQLite"));
    println!("JOURNEY strict image bounds, exact schema/event/provider versions and root/demo secret rejection");

    Ok(())
}
