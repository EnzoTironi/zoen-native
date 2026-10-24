//! Complete authority cutover scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn plan_refresh_reuses_verified_payload_and_rechecks_after_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut session = f.session();
    let initial = session.read(&f.engine).unwrap();
    let mut document = plan_doc(&session.shadow).unwrap().fork().unwrap();
    let revision = document.revision().unwrap();
    document
        .apply_from(
            &revision,
            &order_doc(&document),
            &[text_edit(&initial, "Verified Plan 😀\r\ncontent")],
        )
        .unwrap();
    let bytes = document.full_update().to_json().unwrap();
    let blob = f.engine.page_blob(&f.space, &bytes).unwrap();
    let mut page = f.page(&document);
    page.blob = Some(blob.clone());
    page.loro.clear();
    let event = f.ordered(f.version(ItemContent::Page(page)))?;
    let fold = f.engine.plan_timeline(&f.item).unwrap();
    assert!(fold.acknowledges(
        &event.client_id,
        &plan::source(&f.space, &f.item, &event, true).event_id
    ));
    let before = session.read(&f.engine).unwrap();
    assert!(before.can_edit && before.save_error.is_none());
    assert_ne!(before.content_hash, initial.content_hash);
    let (mime, bytes) = f.engine.store.media(&blob.sha256).unwrap().unwrap();
    assert_eq!(
        f.engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
            .unwrap(),
        1
    );
    let cached = session.read(&f.engine).unwrap();
    assert!(
        cached.can_edit && cached.save_error.is_none(),
        "already verified Plan history must not be downloaded again"
    );
    assert_eq!(cached.view, before.view);
    assert_eq!(cached.content_hash, before.content_hash);

    f.engine.reload().unwrap();
    let reloaded = session.read(&f.engine).unwrap();
    assert!(!reloaded.can_edit);
    assert!(matches!(session.blocked, Some(SessionBlock::Waiting(_))));
    let mut reopened = f.session();
    assert_eq!(
        reloaded.content_hash,
        reopened.read(&f.engine).unwrap().content_hash
    );
    f.engine
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let resumed = session.read(&f.engine).unwrap();
    assert!(resumed.can_edit && resumed.save_error.is_none());
    assert_eq!(resumed.view, before.view);
    assert_eq!(resumed.content_hash, before.content_hash);

    Ok(())
}

pub(super) fn pending_legacy_and_remote_cutover_never_expand_seed(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for undo in [false, true] {
        let mut f = Fixture::new()?;
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        let original_manifest = f.engine.plan_timeline(&f.item).unwrap().manifest().unwrap();
        let body = if undo {
            EventBody::ItemReverted {
                item: f.item.clone(),
                to_version: 1,
                note: "pending undo".into(),
            }
        } else {
            let mut doc = original();
            doc.title = "optimistic legacy title".into();
            f.version(ItemContent::Plan(doc))
        };
        let legacy = f.queue(body);
        let blocked = s.read(&f.engine).unwrap();
        assert!(!blocked.can_edit);
        assert_eq!(s.blocked, Some(SessionBlock::PendingLegacy));
        let cipher = f.cipher();
        assert!(apply(&f, &mut s, &read, "blocked", &[]).is_err());
        assert_eq!(f.cipher(), cipher);
        let fold = f.engine.plan_timeline(&f.item).unwrap();
        assert!(fold.accepts_prefix(&original_manifest));
        let mut remote = fold.document.unwrap().fork().unwrap();
        let revision = remote.revision().unwrap();
        remote
            .apply_from(
                &revision,
                &order_doc(&remote),
                &[text_edit(&read, "ordered remote cutover")],
            )
            .unwrap();
        let page = PageContent {
            title: "cached hint".into(),
            path: String::new(),
            loro: encode_bytes(&remote.full_update().to_json().unwrap()),
            blob: None,
            manifest: Some(PageManifest::PlanV1(original_manifest.clone())),
        };
        f.ordered(f.version(ItemContent::Page(page)))?;
        let fold = f.engine.plan_timeline(&f.item).unwrap();
        assert!(fold.frozen_at.is_some());
        assert!(fold.pending_legacy);
        assert_eq!(
            fold.document.as_ref().unwrap().blocks().unwrap(),
            remote.blocks().unwrap()
        );
        assert_eq!(
            fold.origin_tag().unwrap().2,
            original_manifest.creation_event
        );
        assert!(fold.accepts_prefix(&original_manifest));
        if undo {
            deliver(&mut f.engine, &legacy, Ingest::Confirmed)?;
            assert!(matches!(
                f.engine
                    .plan_timeline(&f.item)
                    .unwrap()
                    .slots
                    .last()
                    .unwrap()
                    .acceptance,
                Acceptance::Refused(_)
            ));
        } else {
            assert!(f
                .engine
                .reject(&legacy.client_id, "legacy writer refused", true, None)
                .user_failed());
        }
        let current = s.read(&f.engine).unwrap();
        assert!(current.can_edit);
        assert_eq!(blocks(&current), remote.blocks().unwrap());
        assert!(f
            .engine
            .plan_timeline(&f.item)
            .unwrap()
            .accepts_prefix(&original_manifest));
    })
}

pub(super) fn late_legacy_versions_never_ack_or_change_canonical_shadow(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "canonical content")],
    )
    .unwrap();
    let reference = queued(s.commit(&mut f.engine, "cutover").unwrap());
    f.ack(&reference)?;
    let canonical = s.read(&f.engine).unwrap();
    let shadow = plan_doc(&s.shadow)
        .unwrap()
        .full_update()
        .to_json()
        .unwrap();
    let cipher = f.cipher();
    let mut legacy = original();
    legacy.title = "late incompatible title".into();
    f.ordered(f.version(ItemContent::Plan(legacy)))?;
    f.ordered(EventBody::ItemReverted {
        item: f.item.clone(),
        to_version: 1,
        note: "late undo".into(),
    })?;
    let fold = f.engine.plan_timeline(&f.item).unwrap();
    assert!(fold
        .slots
        .iter()
        .rev()
        .take(2)
        .all(|slot| matches!(slot.acceptance, Acceptance::Refused(_))));
    assert!(fold
        .slots
        .iter()
        .rev()
        .take(2)
        .all(|slot| slot.content.is_some()));
    assert_eq!(s.read(&f.engine).unwrap().view, canonical.view);
    assert_eq!(
        plan_doc(&s.shadow)
            .unwrap()
            .full_update()
            .to_json()
            .unwrap(),
        shadow
    );
    assert_eq!(f.cipher(), cipher);
    assert!(!fold.acknowledges(
        &fold.slots.last().unwrap().source.client_id,
        &fold.slots.last().unwrap().source.event_id
    ));

    Ok(())
}

pub(super) fn reader_removed_actor_and_key_loss_refuse_mutation_but_keep_saved_read(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(
        for mode in [
            "reader",
            "reader-unchanged",
            "removed",
            "key-loss",
            "pending-grant",
        ] {
            let mut f = Fixture::new()?;
            let mut s = f.session();
            let read = s.read(&f.engine).unwrap();
            let edit = text_edit(&read, "private intent remains");
            apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
            if mode == "reader-unchanged" {
                let undo = apply(&f, &mut s, &read, "local-undo", &[]).unwrap();
                assert_eq!(undo.current.content_hash, read.content_hash);
                assert_eq!(
                    plan_doc(&s.live).unwrap().semantic_hash().unwrap(),
                    plan_doc(&s.shadow).unwrap().semantic_hash().unwrap()
                );
                assert!(journal(&s).has_draft);
                assert!(journal(&s).commit.is_none());
            }
            let author = f.owner.clone();
            let admin = if mode == "pending-grant" {
                let admin = identity(&mut f.engine, "Admin");
                f.ordered(EventBody::MemberAdded {
                    identity: admin.clone(),
                    role: Role::Owner,
                })?;
                Some(admin)
            } else {
                None
            };
            match mode {
                "reader" | "reader-unchanged" | "pending-grant" => {
                    f.ordered(EventBody::MemberAdded {
                        identity: author.clone(),
                        role: Role::Reader,
                    })?;
                }
                "removed" => {
                    f.ordered(EventBody::MemberRemoved {
                        identity: author.clone(),
                    })?;
                }
                "key-loss" => {
                    f.engine.signers.remove(&author);
                }
                _ => unreachable!(),
            }
            if mode == "pending-grant" {
                // A different authenticated owner can queue an optimistic regrant; that
                // projection still must not create ordered authority for this editor.
                let writer = f.engine.author_for(admin.as_ref().unwrap()).unwrap();
                f.engine
                    .append_synced(
                        &f.space,
                        &writer,
                        1_800_000_000_000,
                        EventBody::MemberAdded {
                            identity: author.clone(),
                            role: Role::Member,
                        },
                    )
                    .unwrap();
            }
            let readonly = s.read(&f.engine).unwrap();
            assert!(!readonly.can_edit);
            assert!(readonly.edit_context.is_empty());
            if mode == "reader-unchanged" {
                assert_eq!(readonly.view, read.view);
            } else {
                assert!(blocks(&readonly)
                    .iter()
                    .any(|b| b.plain_text() == "private intent remains"));
            }
            let before = exact(&f, &s);
            assert!(apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).is_err());
            assert!(apply(&f, &mut s, &read, "new", &[edit]).is_err());
            assert!(s.commit(&mut f.engine, "forbidden").is_err());
            no_change(&f, &s, &before);
            assert!(journal(&s).has_draft);
        },
    )
}
