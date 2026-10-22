//! Complete bounds scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn input_limits_foreign_scope_and_authenticated_journal_migration_fail_closed() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        queued(
            session
                .commit(&mut f.engine, "migrate exact pending journal")
                .unwrap(),
        );
        drop(session);
        let original = journal_without_input(&f);
        let (key, aad, tag) = input_journal_crypto(&f);
        let literal = serde_json::to_vec(&original).unwrap();
        // Recreate the authentic pre-slot envelope with this actual device key,
        // origin AAD, snapshot, edit receipts and pending publication.
        let legacy = if kind == ItemKind::Page {
            write_legacy_fixture(&f.engine, f.engine.item_state(&f.item).unwrap(), &literal)
                .unwrap()
        } else {
            let sealed = crate::local_sealed::seal(&key, &aad, &literal).unwrap();
            format!("v2:{}", sealed.strip_prefix("v1:").unwrap())
        };
        store_input_fixture(&f, &legacy);
        f = reopen_input_fixture(f);
        let scope = input_scope(&f);
        let empty = f.engine.page_input_load(&scope).unwrap();
        assert_eq!(empty.revision, 0);
        assert!(empty.capture.is_none());
        assert_eq!(
            f.cipher().as_deref(),
            Some(legacy.as_str()),
            "authenticated read must not migrate by writing"
        );
        assert_eq!(journal_without_input(&f), original);
        let opened = input_open(&f, WRITER_A);
        assert!(f.cipher().unwrap().starts_with("v3:"));
        assert_eq!(journal_without_input(&f), original);
        let payload = rich_payload(7, "😀e\u{301}");
        let accepted = f
            .engine
            .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
            .unwrap();
        let cipher = f.cipher();
        let journal = journal_without_input(&f);
        let mut invalids = vec![vec![b'x'; MAX_CAPTURE + 1]];
        let valid = rich_payload(8, "😀e\u{301}");
        for (field, replacement) in [
            ("schema", serde_json::json!("unknown/2")),
            ("mode", serde_json::json!("unknown-mode")),
            ("selection", serde_json::json!({"start":1,"end":2})),
            ("composition", serde_json::json!({"start":0,"end":1})),
            ("sequence", serde_json::json!("08")),
            (
                "inline",
                serde_json::json!([{"start":0,"end":2,"type":"strong","url":null,"origin":"future"}]),
            ),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&valid).unwrap();
            value[field] = replacement;
            invalids.push(serde_json::to_vec(&value).unwrap());
        }
        let raw = String::from_utf8(valid.clone()).unwrap();
        invalids.push(format!("{{\"schema\":\"enrm-ios/1\",{}", &raw[1..]).into_bytes());
        let mut unknown: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        unknown["future"] = serde_json::json!(true);
        invalids.push(serde_json::to_vec(&unknown).unwrap());
        let mut ranges: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        ranges["inline"] = serde_json::Value::Array(vec![
            serde_json::json!({"start":0,"end":2,"type":"strong","url":null,"origin":"stored"});
            8193
        ]);
        invalids.push(serde_json::to_vec(&ranges).unwrap());
        for bytes in invalids {
            assert!(f
                .engine
                .page_input_checkpoint(&scope, WRITER_A, accepted.current.revision, 8, &bytes)
                .is_err());
            assert_eq!(f.cipher(), cipher);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), accepted.current);
        }
        let mut scopes = vec![scope.clone(); 4];
        scopes[0].expected_identity_id = "foreign-identity".into();
        scopes[1].expected_device_id = "foreign-device".into();
        scopes[2].space_id = "foreign-space".into();
        scopes[3].item_id = "foreign-item".into();
        for foreign in scopes {
            assert!(f.engine.page_input_load(&foreign).is_err());
            assert!(f
                .engine
                .page_input_checkpoint(&foreign, WRITER_A, accepted.current.revision, 8, &valid)
                .is_err());
            assert_eq!(f.cipher(), cipher);
        }
        assert!(f
            .engine
            .page_input_open(
                &scope,
                accepted.current.revision,
                WRITER_B,
                accepted.current.binding.as_ref().unwrap().target.clone(),
                PageInputMode::Markdown,
                &accepted.current.binding.as_ref().unwrap().base_edit_context
            )
            .is_err());
        let unlocked = f.engine.net.author.take();
        let root_key = f.engine.signers.remove(&f.owner);
        assert!(f.engine.page_input_load(&scope).is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine.net.author = unlocked;
        if let Some(root_key) = root_key {
            f.engine.signers.insert(f.owner.clone(), root_key);
        }
        assert_eq!(journal_without_input(&f), journal);
        // Backup still excludes the whole private row after it gains input bytes.
        let backup = Database::new();
        std::fs::write(backup.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(backup.path()).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'page.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        drop(conn);
        assert_eq!(f.cipher(), cipher);
        if kind == ItemKind::Page {
            // Both inputs fit alone. Their aggregate must be refused without
            // replacing the genuine, already retained native capture.
            let large_payload = rich_payload(8, &"x".repeat(MAX_CAPTURE - 1024));
            assert!(large_payload.len() <= MAX_CAPTURE);
            let large_ack = f
                .engine
                .page_input_checkpoint(
                    &scope,
                    WRITER_A,
                    accepted.current.revision,
                    8,
                    &large_payload,
                )
                .unwrap();
            let large_cipher = f.cipher();
            let s = f.session();
            let page = s.binary().unwrap();
            let huge = page.at(&page.frontiers()).unwrap();
            let mut seed = 0x9e3779b97f4a7c15u64;
            let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";
            let description: String = (0..1024 * 1024)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    alphabet[(seed & 63) as usize] as char
                })
                .collect();
            let blocks = huge.blocks();
            let id = new_id("oversized-input-block");
            let mut order: Vec<_> = blocks.iter().map(|b| b.id.clone()).collect();
            order.push(id.clone());
            huge.apply(
                &order,
                &[documents::BlockEdit {
                    // Image metadata uses a real CRDT value without creating
                    // millions of rich-text cursor IDs unrelated to this bound.
                    id,
                    kind: Kind::Image {
                        url: "https://example.test/large-asset.png".into(),
                        alt: description,
                        title: String::new(),
                    },
                    text: String::new(),
                    spans: vec![],
                }],
            )
            .unwrap();
            let SessionJournal::BinaryPage(mut candidate) = s.journal else {
                unreachable!()
            };
            candidate.snapshot = Some(encode_bytes(&huge.snapshot()));
            let retained_input = std::mem::take(&mut candidate.input);
            let without_input_bytes = serde_json::to_vec(&candidate).unwrap().len();
            assert!(
                without_input_bytes < MAX_JOURNAL_BYTES,
                "snapshot alone: {without_input_bytes}"
            );
            candidate.input = retained_input;
            let aggregate_bytes = serde_json::to_vec(&candidate).unwrap().len();
            assert!(
                aggregate_bytes > MAX_JOURNAL_BYTES,
                "aggregate: {aggregate_bytes}, snapshot alone: {without_input_bytes}"
            );
            assert!(f
                .engine
                .save_page_journal(f.engine.item_state(&f.item).unwrap(), &candidate)
                .is_err());
            assert_eq!(f.cipher(), large_cipher);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), large_ack.current);
        }
        if kind == ItemKind::Plan {
            let mut foreign_origin = original.clone();
            foreign_origin["tag"]["creation_event"] =
                serde_json::json!("another-authenticated-creation");
            let foreign_origin = seal_v3(
                &key,
                &aad,
                &serde_json::to_vec(&foreign_origin).unwrap(),
                tag,
            )
            .unwrap();
            store_input_fixture(&f, &foreign_origin);
            f = reopen_input_fixture(f);
            assert!(f.engine.page_input_load(&scope).is_err());
            assert_eq!(f.cipher().as_deref(), Some(foreign_origin.as_str()));
        }
        // Unknown fields in an authenticated v3 future slot stay locked and
        // intact instead of falling back to an empty/default journal.
        let mut future = original.clone();
        future["input"] = serde_json::json!({"revision":0,"binding":null,"capture":null,"receipts":[],"future_format":2});
        let future = seal_v3(&key, &aad, &serde_json::to_vec(&future).unwrap(), tag).unwrap();
        store_input_fixture(&f, &future);
        f = reopen_input_fixture(f);
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        assert!(!read.can_edit);
        assert!(matches!(s.blocked, Some(SessionBlock::DraftLocked(_))));
        assert!(f.engine.page_input_load(&scope).is_err());
        assert_eq!(f.cipher().as_deref(), Some(future.as_str()));
        // Restore the genuine retained row, then revoke writing through a real
        // ordered membership event rather than mutating a cached permission.
        store_input_fixture(&f, cipher.as_deref().unwrap());
        f = reopen_input_fixture(f);
        f.ordered(EventBody::MemberAdded {
            identity: f.owner.clone(),
            role: Role::Reader,
        });
        assert!(f.engine.page_input_load(&scope).is_err());
        assert!(f
            .engine
            .page_input_checkpoint(&scope, WRITER_A, accepted.current.revision, 8, &valid)
            .is_err());
        assert_eq!(f.cipher(), cipher);
    }
}
