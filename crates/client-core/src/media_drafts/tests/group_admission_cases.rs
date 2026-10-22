//! Complete group admission scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn media_contract_group_dependencies_wait_for_confirmed_files_and_do_not_block_other_spaces(
) {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f);
    let other_space = f
        .engine
        .create_group_with("Other space".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let other = Fixture {
        engine: f.engine.clone(),
        vault: f.vault.clone(),
        scope: MediaDraftScope {
            space_id: other_space.clone(),
            ..f.scope.clone()
        },
    };
    media_contract_confirm_scope(&other);
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let independent = f
        .engine
        .send_message(other_space, "Unrelated space stays live".into())
        .unwrap();
    let waiting_for_upload = f.engine.lock().outbox_envelopes();
    assert!(
        waiting_for_upload
            .iter()
            .any(|env| env.client_id() == independent.id),
        "a different Space must not wait for this upload"
    );
    assert!(
        waiting_for_upload
            .iter()
            .all(|env| env.space() != f.scope.space_id),
        "this Space waits while actual upload is pending"
    );
    media_contract_upload_all(&f);
    let offered = f.engine.lock().outbox_envelopes();
    let first_file = f
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
            ) && row.event.space == f.scope.space_id
        })
        .unwrap()
        .event
        .client_id;
    assert!(offered.iter().any(|env| env.client_id() == first_file));
    assert!(offered.iter().any(|env| env.client_id() == independent.id));
    assert!(sent
        .entries
        .iter()
        .all(|entry| !offered.iter().any(|env| env.client_id() == entry.id)));
    let written = offered
        .iter()
        .map(|env| (env.client_id().to_string(), None))
        .collect();
    assert!(
        f.engine
            .lock()
            .outbox_envelopes_except(&written)
            .iter()
            .all(|env| sent.entries.iter().all(|entry| env.client_id() != entry.id)),
        "a socket write cannot confirm the referenced File"
    );
    media_contract_confirm_id(&f, &first_file);
    let offered = f.engine.lock().outbox_envelopes();
    assert!(offered
        .iter()
        .any(|env| env.client_id() == sent.entries[0].id));
    assert!(!offered
        .iter()
        .any(|env| env.client_id() == sent.entries[1].id));
    let second_file = offered
        .iter()
        .find(|env| env.client_id() != sent.entries[0].id && env.client_id() != independent.id)
        .unwrap()
        .client_id()
        .to_string();
    media_contract_confirm_id(&f, &second_file);
    let written = HashMap::from([(sent.entries[0].id.clone(), None)]);
    let offered = f.engine.lock().outbox_envelopes_except(&written);
    assert!(
        !offered
            .iter()
            .any(|env| env.client_id() == sent.entries[1].id),
        "confirmed File bytes and a sent predecessor still do not confirm its group position"
    );
    assert!(offered.iter().any(|env| env.client_id() == independent.id));
    media_contract_confirm_id(&f, &sent.entries[0].id);
    assert!(f
        .engine
        .lock()
        .outbox_envelopes()
        .iter()
        .any(|env| env.client_id() == sent.entries[1].id));
}

pub(super) fn media_contract_grouped_preexisting_confirmed_files_need_no_new_creation() {
    let f = fixture(":memory:");
    let items: Vec<_> = ["one.pdf", "two.pdf"]
        .into_iter()
        .map(|name| {
            f.engine
                .lock()
                .file_add(
                    &f.scope.space_id,
                    "",
                    name,
                    "application/pdf",
                    name.as_bytes(),
                    None,
                )
                .unwrap()
        })
        .collect();
    media_contract_upload_all(&f);
    media_contract_confirm_scope(&f);
    let original_file = f.engine.file_info_at(items[0].clone(), None).unwrap();
    media_contract_receive(
        &f,
        types::EventBody::ItemCreated {
            item: items[0].clone(),
            kind: types::ItemKind::Note,
            content: types::ItemContent::Text {
                text: "Refused duplicate creation".into(),
            },
            origin: String::new(),
        },
        false,
    );
    assert_eq!(
        f.engine.file_info_at(items[0].clone(), None).unwrap(),
        original_file,
        "a signed but projection-refused duplicate cannot replace the confirmed File"
    );
    let options = types::AttachmentSendOptions {
        grouping: types::AttachmentGrouping::Grouped,
        ..Default::default()
    };
    let ids: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            f.engine
                .lock()
                .append(
                    &f.scope.space_id,
                    &f.scope.expected_identity_id,
                    types::EventBody::MessagePosted {
                        message: types::new_id("msg"),
                        text: format!("Existing {index}"),
                        attaches: Some(item.clone()),
                        media: options.metadata(
                            &f.scope.expected_identity_id,
                            &f.scope.space_id,
                            None,
                            false,
                            &["existing-one", "existing-two"],
                            index,
                        ),
                        reply: None,
                        forwarded: false,
                    },
                )
                .unwrap()
                .client_id
        })
        .collect();
    let offered = f.engine.lock().outbox_envelopes();
    assert_eq!(
        offered
            .iter()
            .map(|env| env.client_id())
            .collect::<Vec<_>>(),
        [ids[0].as_str()]
    );
    media_contract_confirm_id(&f, &ids[0]);
    assert_eq!(
        f.engine
            .lock()
            .outbox_envelopes()
            .iter()
            .map(|env| env.client_id().to_string())
            .collect::<Vec<_>>(),
        [ids[1].clone()]
    );
}

pub(super) fn media_contract_rejects_invalid_options_and_foreign_authority_without_mutation() {
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    for options in [
        serde_json::json!({"presentation":"file","grouping":"grouped"}),
        serde_json::json!({"presentation":"media","grouping":"guessed"}),
        serde_json::json!({"presentation":"fake_mime","grouping":"separate"}),
        serde_json::json!({"presentation":"media","grouping":"separate","quality":"original"}),
        serde_json::json!({"grouping":"separate"}),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&attachments(&["a"], None)).unwrap();
        value["options"] = options;
        assert!(f
            .engine
            .save_media_draft(
                f.scope.clone(),
                "0".into(),
                value.to_string(),
                vec![input("a")]
            )
            .is_err());
        assert_eq!(counts(&f.engine), before);
    }
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let mut foreign = f.scope.clone();
    foreign.expected_identity_id = event_log::Signer::generate().id();
    let before = counts(&f.engine);
    assert!(f.engine.send_media_draft(foreign, draft.revision).is_err());
    assert_eq!(counts(&f.engine), before);
}

pub(super) fn media_contract_rejects_invalid_group_index_and_duplicate_membership_atomically() {
    let f = fixture(":memory:");
    let options = types::AttachmentSendOptions {
        presentation: types::AttachmentPresentation::File,
        grouping: types::AttachmentGrouping::Grouped,
    };
    let before = counts(&f.engine);
    let invalid = options.metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["one", "two"],
        1,
    );
    assert!(f
        .engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "a.jpg",
            "image/jpeg",
            &[1, 2, 3],
            None,
            None,
            false,
            "one",
            "",
            invalid
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
    let first = options.metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["one", "two"],
        0,
    );
    f.engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "a.jpg",
            "image/jpeg",
            &[1, 2, 3],
            None,
            None,
            false,
            "one",
            "",
            first.clone(),
        )
        .unwrap();
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .send_file_message_with_media(
            &f.scope.space_id,
            "b.jpg",
            "image/jpeg",
            &[4, 5, 6],
            None,
            None,
            false,
            "other",
            "",
            first
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
}

pub(super) fn media_contract_forwarded_group_is_rejected_and_single_forward_preserves_file_presentation(
) {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &media_options(&["a", "b"], "file", "grouped"),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let forwarded = f
        .engine
        .lock()
        .forward(&f.scope.space_id, &sent.entries[0].id, &f.scope.space_id)
        .unwrap();
    assert_eq!(
        forwarded.media.as_ref().unwrap().presentation,
        crate::AttachmentPresentationDto::File
    );
    assert!(forwarded.media.as_ref().unwrap().group.is_none());
    assert!(forwarded.forwarded);
    let item = f
        .engine
        .lock()
        .file_add(
            &f.scope.space_id,
            "new",
            "new.pdf",
            "application/pdf",
            &[7, 8, 9],
            None,
        )
        .unwrap();
    let metadata = types::AttachmentSendOptions {
        presentation: types::AttachmentPresentation::File,
        grouping: types::AttachmentGrouping::Grouped,
    }
    .metadata(
        &f.scope.expected_identity_id,
        &f.scope.space_id,
        None,
        false,
        &["new-1", "new-2"],
        0,
    );
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            types::EventBody::MessagePosted {
                message: types::new_id("msg"),
                text: String::new(),
                attaches: Some(item),
                media: metadata,
                reply: None,
                forwarded: true,
            }
        )
        .is_err());
    assert_eq!(counts(&f.engine), before);
}
