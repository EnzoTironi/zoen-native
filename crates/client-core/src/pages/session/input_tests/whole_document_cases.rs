//! Whole-document source edits use the real encrypted journal and observed CRDT context.
use super::*;

fn markdown_payload(sequence: u64, text: &str) -> Vec<u8> {
    let end = text.encode_utf16().count();
    serde_json::to_vec(&serde_json::json!({
        "schema":"page-text/1", "sequence":sequence.to_string(), "mode":"markdown",
        "text":text, "selection":{"start":end,"end":end}, "composition":null
    }))
    .unwrap()
}

fn open_whole(f: &Fixture, writer: &str, context: &str) -> PageInputDto {
    let input = f.engine.page_input_load(&input_scope(f)).unwrap();
    f.engine
        .page_input_open(
            &input_scope(f),
            input.revision,
            writer,
            PageInputTarget::WholeDocument,
            PageInputMode::Markdown,
            context,
        )
        .unwrap()
}

fn seed_document(f: &Fixture, markdown: &str) -> crate::pages::PageDto {
    let before = f.engine.page(&f.item).unwrap();
    let blocks = crate::pages::parse_editor_markdown(markdown).unwrap();
    f.engine
        .page_apply_from(
            &f.item,
            "seed-whole-document",
            &before.edit_context,
            &blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
            &blocks,
        )
        .unwrap()
        .page
}

#[test]
fn whole_document_noop_preserves_duplicate_ids_and_unexpressed_metadata(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, f) = cold_input_fixture(ItemKind::Page)?;
    let page = seed_document(
        &f,
        "Title\n=====\n\nSame\n\nSame\n\n![alt](https://example.test/image \"title\")\n",
    );
    assert_eq!(page.blocks[1].text, page.blocks[2].text);
    assert_ne!(page.blocks[1].id, page.blocks[2].id);
    assert_eq!(page.blocks[0].setext, Some(true));
    assert_eq!(page.blocks[3].title.as_deref(), Some("title"));
    let opened = open_whole(&f, WRITER_A, &page.edit_context);
    let binding = opened.binding.as_ref().unwrap();
    assert_eq!(binding.target, PageInputTarget::WholeDocument);
    assert_eq!(binding.base_block_json, "null");
    let seed = binding.base_document_markdown.as_ref().unwrap();
    assert_eq!(seed, &f.engine.page_markdown(&f.item).unwrap());
    let scope = input_scope(&f);
    let captured = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_A,
            opened.revision,
            1,
            &markdown_payload(1, seed),
        )
        .unwrap();
    assert!(
        captured
            .current
            .binding
            .as_ref()
            .unwrap()
            .base_document_markdown
            .is_none(),
        "status/checkpoint metadata does not repeatedly publish the document seed"
    );
    let capture = captured.current.capture.as_ref().unwrap();
    let applied = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            1,
            &capture.sha256,
            "whole-noop",
        )
        .unwrap();
    assert_eq!(applied.edit.page.blocks, page.blocks);
    Ok(())
}

#[test]
fn whole_document_cold_recovery_sql_failure_and_lost_ack_have_one_private_acceptance(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, mut f) = cold_input_fixture(ItemKind::Page)?;
    let page = seed_document(&f, "# Before\n\nKeep 😀\n\nRemove\n");
    let events = f.engine.store.event_count().unwrap();
    let outbox = f.engine.store.outbox_len().unwrap();
    let scope = input_scope(&f);
    let opened = open_whole(&f, WRITER_A, &page.edit_context);
    let payload = markdown_payload(7, "# After\n\nKeep 😀\n\nNew\n");
    let captured = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .unwrap();
    f = reopen_input_fixture(f);
    assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
    let resumed = open_whole(&f, WRITER_B, &page.edit_context);
    assert_eq!(resumed.capture.as_ref().unwrap().payload, payload);
    assert_eq!(
        resumed.binding.as_ref().unwrap().base_document_markdown,
        opened.binding.as_ref().unwrap().base_document_markdown
    );
    let capture = resumed.capture.as_ref().unwrap();
    let before = f.cipher();
    trigger(
        &f,
        "refuse_whole_apply",
        "INSERT",
        "meta",
        "WHEN NEW.key LIKE 'page.draft.%'",
    );
    assert!(f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            resumed.revision,
            7,
            &capture.sha256,
            "whole-apply",
        )
        .is_err());
    assert_eq!(f.cipher(), before);
    assert_eq!(f.engine.page(&f.item).unwrap().blocks, page.blocks);
    drop_trigger(&f, "refuse_whole_apply");
    f.engine
        .with_input(&scope, |session| {
            session.lose_apply_response = true;
            Ok(())
        })
        .unwrap();
    assert!(f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            resumed.revision,
            7,
            &capture.sha256,
            "whole-apply",
        )
        .is_err());
    f = reopen_input_fixture(f);
    let replay = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            resumed.revision,
            7,
            &capture.sha256,
            "whole-apply",
        )
        .unwrap();
    assert!(replay.ack.replayed && replay.edit.replayed);
    let blocks = &replay.edit.page.blocks;
    assert_eq!(
        blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        ["After", "Keep 😀", "New"]
    );
    assert_eq!(
        blocks[1], page.blocks[1],
        "unchanged block retains its identity and metadata"
    );
    assert!(!blocks
        .iter()
        .any(|b| b.id == page.blocks[0].id || b.id == page.blocks[2].id));
    assert_eq!(f.engine.page(&f.item).unwrap().blocks, *blocks);
    assert_eq!(f.engine.store.event_count().unwrap(), events);
    assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
    assert!(f.engine.page_input_load(&scope).unwrap().capture.is_none());
    Ok(())
}

#[test]
fn whole_document_observed_seed_and_merge_preserve_concurrent_unobserved_changes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, f) = cold_input_fixture(ItemKind::Page)?;
    let page = seed_document(&f, "# Original\n\nKeep\n");
    let mut changed = page.blocks[1].clone();
    changed.text = "Concurrent text".into();
    let mut inserted = changed.clone();
    inserted.id = "concurrent-new-block".into();
    inserted.text = "Concurrent insertion".into();
    let mut order = page.blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
    order.push(inserted.id.clone());
    f.engine
        .page_apply_from(
            &f.item,
            "concurrent-edit",
            &page.edit_context,
            &order,
            &[changed.clone(), inserted.clone()],
        )
        .unwrap();
    let opened = open_whole(&f, WRITER_A, &page.edit_context);
    let seed = opened
        .binding
        .as_ref()
        .unwrap()
        .base_document_markdown
        .as_ref()
        .unwrap();
    assert!(
        seed.contains("Keep") && !seed.contains("Concurrent"),
        "seed is the exact observed context, never the newer live document"
    );
    let scope = input_scope(&f);
    let captured = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_A,
            opened.revision,
            1,
            &markdown_payload(1, &format!("{seed}\nAdded from source\n")),
        )
        .unwrap();
    let capture = captured.current.capture.as_ref().unwrap();
    let applied = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            1,
            &capture.sha256,
            "merge-whole-input",
        )
        .unwrap();
    let blocks = &applied.edit.page.blocks;
    assert_eq!(
        blocks.iter().find(|b| b.id == changed.id),
        Some(&changed),
        "an unchanged observed block cannot overwrite a concurrent edit"
    );
    assert!(
        blocks.contains(&inserted),
        "an unobserved insertion cannot be deleted"
    );
    assert_eq!(
        blocks
            .iter()
            .filter(|b| b.text == "Added from source")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn native_block_input_does_not_inherit_another_editor_continuation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, f) = cold_input_fixture(ItemKind::Page)?;
    let page = seed_document(&f, "# Original\n\nOther\n");
    let mut concurrent = page.blocks[1].clone();
    concurrent.text = "Concurrent block change".into();
    f.engine
        .page_apply_from(
            &f.item,
            "other-editor",
            &page.edit_context,
            &page.blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
            std::slice::from_ref(&concurrent),
        )
        .unwrap();
    let scope = input_scope(&f);
    let input = f.engine.page_input_load(&scope).unwrap();
    let opened = f
        .engine
        .page_input_open(
            &scope,
            input.revision,
            WRITER_A,
            PageInputTarget::Block {
                block_id: page.blocks[0].id.clone(),
            },
            PageInputMode::Markdown,
            &page.edit_context,
        )
        .unwrap();
    let captured = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_A,
            opened.revision,
            1,
            &markdown_payload(1, "# Changed natively"),
        )
        .unwrap();
    let capture = captured.current.capture.as_ref().unwrap();
    let applied = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            1,
            &capture.sha256,
            "apply-distinct-native-editor",
        )
        .unwrap();
    let blocks = &applied.edit.page.blocks;
    assert_eq!(
        blocks.iter().find(|b| b.id == concurrent.id),
        Some(&concurrent)
    );
    assert_eq!(
        blocks
            .iter()
            .find(|b| b.id == page.blocks[0].id)
            .unwrap()
            .text,
        "Changed natively"
    );
    Ok(())
}

#[test]
fn whole_document_rejects_wrong_modes_plan_targets_and_foreign_scope_without_writes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, f) = cold_input_fixture(kind)?;
        let scope = input_scope(&f);
        let context = f.session().read(&f.engine).unwrap().edit_context;
        let input = f.engine.page_input_load(&scope).unwrap();
        let before = f.cipher();
        for mode in [PageInputMode::RawRich, PageInputMode::LosslessText] {
            assert!(f
                .engine
                .page_input_open(
                    &scope,
                    input.revision,
                    WRITER_A,
                    PageInputTarget::WholeDocument,
                    mode,
                    &context
                )
                .is_err());
            assert_eq!(f.cipher(), before);
        }
        if kind == ItemKind::Plan {
            assert!(f
                .engine
                .page_input_open(
                    &scope,
                    input.revision,
                    WRITER_A,
                    PageInputTarget::WholeDocument,
                    PageInputMode::Markdown,
                    &context
                )
                .is_err());
            assert_eq!(f.cipher(), before);
        } else {
            let mut foreign = scope.clone();
            foreign.expected_device_id = "another-device".into();
            assert!(f
                .engine
                .page_input_open(
                    &foreign,
                    input.revision,
                    WRITER_A,
                    PageInputTarget::WholeDocument,
                    PageInputMode::Markdown,
                    &context
                )
                .is_err());
            assert_eq!(f.cipher(), before);
        }
    }
    Ok(())
}

#[test]
fn whole_document_discard_preserves_page_and_empty_apply_removes_only_observed_blocks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, f) = cold_input_fixture(ItemKind::Page)?;
    let page = seed_document(&f, "# Delete locally\n\nExisting\n");
    let scope = input_scope(&f);
    let opened = open_whole(&f, WRITER_A, &page.edit_context);
    let captured = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_A,
            opened.revision,
            1,
            &markdown_payload(1, ""),
        )
        .unwrap();
    let capture = captured.current.capture.as_ref().unwrap();
    assert!(
        f.engine
            .page_input_open(
                &scope,
                captured.current.revision,
                WRITER_B,
                PageInputTarget::AppendNew,
                PageInputMode::Markdown,
                &page.edit_context
            )
            .is_err(),
        "null block id does not make AppendNew equivalent to WholeDocument"
    );
    f.engine
        .page_input_discard(
            &scope,
            WRITER_A,
            captured.current.revision,
            1,
            &capture.sha256,
            "discard-empty-source",
        )
        .unwrap();
    assert_eq!(f.engine.page(&f.item).unwrap().blocks, page.blocks);
    let opened = open_whole(&f, WRITER_B, &page.edit_context);
    let captured = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_B,
            opened.revision,
            1,
            &markdown_payload(1, ""),
        )
        .unwrap();
    let capture = captured.current.capture.as_ref().unwrap();
    let applied = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            captured.current.revision,
            1,
            &capture.sha256,
            "apply-empty-source",
        )
        .unwrap();
    assert!(applied.edit.page.blocks.is_empty());
    Ok(())
}
