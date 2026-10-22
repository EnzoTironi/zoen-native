//! Complete copy recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn failed_media_recovery_copies_single_or_whole_signed_album_without_sending_or_reusing_intents(
) {
    for ids in [vec!["a"], vec!["a", "b", "c"]] {
        let f = fixture(":memory:");
        let sent = refused(&f, &ids, if ids.len() > 1 { "grouped" } else { "separate" });
        let before = public_state(&f);
        let counts_before = counts(&f.engine);
        let draft = recover(&f, &sent.entries.last().unwrap().id).unwrap();
        assert!(!draft.attempted);
        assert_eq!(draft.assets.len(), ids.len());
        assert_eq!(
            public_state(&f),
            before,
            "recovery cannot rewrite Failed history, receipts or outbox"
        );
        assert_eq!(
            counts(&f.engine),
            counts_before,
            "only existing encrypted journal rows may be added"
        );
        let Content::Attachments { items, options, .. } =
            serde_json::from_str::<Content>(&draft.content_json).unwrap()
        else {
            panic!()
        };
        assert_eq!(options.presentation, types::AttachmentPresentation::File);
        assert_eq!(
            options.grouping,
            if ids.len() > 1 {
                types::AttachmentGrouping::Grouped
            } else {
                types::AttachmentGrouping::Separate
            }
        );
        for ((item, descriptor), original) in items.iter().zip(&draft.assets).zip(&ids) {
            assert_ne!(item.asset_id, *original);
            assert_ne!(item.mutation_id, format!("send-{original}"));
            assert_eq!(item.caption, format!("Private caption {original}"));
            assert_eq!(asset(&f, &draft, &descriptor.id), input(original).bytes);
            assert!(rows(&f)
                .iter()
                .all(|(_, value)| !value.contains("Private media bytes")));
        }
        assert!(
            recover(&f, &sent.entries[0].id).is_err(),
            "a second recovery cannot replace an occupied draft"
        );
        assert_eq!(read(&f), draft);
        let resent = f
            .engine
            .send_media_draft(f.scope.clone(), draft.revision.clone())
            .unwrap();
        assert_eq!(resent.entries.len(), ids.len());
        for entry in &resent.entries {
            assert!(!sent.entries.iter().any(|old| old.id == entry.id));
        }
        for original in &sent.entries {
            assert_eq!(
                f.engine
                    .timeline(f.scope.space_id.clone())
                    .unwrap()
                    .iter()
                    .find(|row| row.id == original.id)
                    .unwrap()
                    .delivery,
                crate::Delivery::Failed
            );
        }
    }
}

pub(super) fn failed_media_recovery_cas_keeps_normal_and_media_drafts_and_rejects_stale_revisions()
{
    for case in [
        "normal",
        "reply",
        "media",
        "normal_revision",
        "media_revision",
    ] {
        let f = fixture(":memory:");
        let sent = refused(&f, &["a"], "separate");
        let expected = read(&f).revision;
        let mut normal_revision = "0".to_string();
        match case {
            "normal" => {
                f.engine
                    .lock()
                    .save_message_draft(&f.scope.normal(), "0", "Keep this typing".into(), None)
                    .unwrap();
            }
            "reply" => {
                let root = f
                    .engine
                    .send_message(f.scope.space_id.clone(), "Root".into())
                    .unwrap();
                f.engine
                    .lock()
                    .save_message_draft(&f.scope.normal(), "0", String::new(), Some(root.id))
                    .unwrap();
            }
            "media" => {
                save(
                    &f,
                    &expected,
                    &attachments(&["new"], None),
                    vec![input("new")],
                );
            }
            "normal_revision" => normal_revision = "stale".into(),
            _ => {}
        }
        let before = public_state(&f);
        let private = rows(&f);
        let expected = if case == "media_revision" {
            "stale"
        } else {
            &expected
        };
        assert!(
            f.engine
                .lock()
                .recover_failed_media_draft(
                    &f.scope,
                    &sent.entries[0].id,
                    expected,
                    &normal_revision
                )
                .is_err(),
            "{case}"
        );
        assert_eq!(rows(&f), private, "{case}");
        assert_eq!(public_state(&f), before, "{case}");
    }
}

pub(super) fn failed_media_recovery_preserves_the_signed_inline_or_thread_destination() {
    for thread in [false, true] {
        let mut f = fixture(":memory:");
        media_contract_confirm_scope(&f);
        let root = f
            .engine
            .send_message(f.scope.space_id.clone(), "Synthetic reply root".into())
            .unwrap();
        media_contract_confirm_scope(&f);
        f.scope.thread_root_id = thread.then(|| root.id.clone());
        let mut content: serde_json::Value =
            serde_json::from_str(&media_options(&["a"], "file", "separate")).unwrap();
        content["reply_to"] = (!thread).then_some(root.id.as_str()).into();
        let draft = save(&f, "0", &content.to_string(), vec![input("a")]);
        let sent = f
            .engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap();
        let file = f
            .engine
            .lock()
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|row| {
                matches!(
                    &row.event.body,
                    types::EventBody::ItemCreated {
                        kind: types::ItemKind::File,
                        ..
                    }
                )
            })
            .unwrap()
            .event
            .client_id;
        assert!(f
            .engine
            .lock()
            .reject(&file, "synthetic refusal", true, None)
            .user_failed());
        let recovered = recover(&f, &sent.entries[0].id).unwrap();
        let Content::Attachments { reply_to, .. } =
            serde_json::from_str::<Content>(&recovered.content_json).unwrap()
        else {
            panic!()
        };
        assert_eq!(reply_to, (!thread).then_some(root.id));
        assert_eq!(f.scope.thread_root_id.is_some(), thread);
    }
}

pub(super) fn failed_media_recovery_reopens_the_same_encrypted_copy_and_original_failed_history() {
    let dir = temp();
    let path = dir.join("recovery.sqlite");
    let f = fixture(path.to_str().unwrap());
    let sent = refused(&f, &["a", "b"], "grouped");
    let recovered = recover(&f, &sent.entries[0].id).unwrap();
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(reopened.unlock(vault).unwrap());
    assert_eq!(reopened.media_draft(scope.clone()).unwrap(), recovered);
    for original in &sent.entries {
        assert_eq!(
            reopened
                .timeline(scope.space_id.clone())
                .unwrap()
                .iter()
                .find(|row| row.id == original.id)
                .unwrap()
                .delivery,
            crate::Delivery::Failed
        );
    }
    for (asset, id) in recovered.assets.iter().zip(["a", "b"]) {
        assert_eq!(
            reopened
                .media_draft_asset(scope.clone(), recovered.revision.clone(), asset.id.clone())
                .unwrap(),
            input(id).bytes
        );
    }
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) async fn failed_media_recovery_json_api_keeps_integer_wire_contract_and_rejects_extra_fields(
) {
    let f = fixture(":memory:");
    let sent = refused(&f, &["a"], "separate");
    let mut request = serde_json::json!({"op":"recover_failed_media_draft","scope":f.scope,"message_id":sent.entries[0].id,"expected_revision":read(&f).revision,"expected_message_revision":"0"});
    request["send"] = true.into();
    assert!(f.engine.client_request(request.to_string()).await.is_err());
    request.as_object_mut().unwrap().remove("send");
    let result: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(request.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        result["assets"][0]["size"],
        input("a").bytes.len().to_string()
    );
    assert_eq!(result["attempted"], false);
}
