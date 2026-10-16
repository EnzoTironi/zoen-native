use super::*;

fn edit(id: &str, text: &str) -> BlockEdit {
    BlockEdit {
        id: id.into(),
        kind: Kind::Paragraph,
        text: text.into(),
        spans: Vec::new(),
    }
}

fn order(page: &Page) -> Vec<String> {
    page.blocks().into_iter().map(|b| b.id).collect()
}

fn fixture() -> Page {
    let page = Page::new();
    page.apply(
        &["a".into(), "b".into(), "c".into()],
        &[edit("a", "Alpha"), edit("b", "Bravo"), edit("c", "Charlie")],
    )
    .unwrap();
    page
}

#[test]
fn concurrent_insertion_of_the_same_external_id_cannot_corrupt_the_page() {
    let page = fixture();
    let first = page.at(&page.frontiers()).unwrap();
    let second = page.at(&page.frontiers()).unwrap();
    let mut ids = order(&page);
    ids.push("shared-id".into());
    first.apply(&ids, &[edit("shared-id", "First")]).unwrap();
    second.apply(&ids, &[edit("shared-id", "Second")]).unwrap();
    page.import(&first.updates_since(&page.version())).unwrap();
    let before = page.snapshot();
    let hash = page.content_hash();
    assert!(page.import(&second.updates_since(&page.version())).is_err());
    assert_eq!(page.snapshot(), before);
    assert_eq!(page.content_hash(), hash);
    assert_eq!(
        page.blocks().iter().filter(|b| b.id == "shared-id").count(),
        1
    );
    let mut edited = edit("shared-id", "Still editable");
    edited.spans.push(Span {
        start: 0,
        end: 5,
        key: "b".into(),
        value: String::new(),
    });
    page.apply(&ids, &[edited]).unwrap();
}

fn assert_refused_without_mutation(page: &Page, ids: &[String], edits: &[BlockEdit]) {
    let markdown = page.to_markdown();
    let blocks = page.blocks();
    let snapshot = page.snapshot();
    let frontiers = page.frontiers();
    let version = page.version();
    let hash = page.content_hash();
    assert!(page.apply(ids, edits).is_err());
    assert_eq!(page.to_markdown(), markdown);
    assert_eq!(page.blocks(), blocks);
    assert_eq!(page.snapshot(), snapshot);
    assert_eq!(page.frontiers(), frontiers);
    assert_eq!(page.version(), version);
    assert_eq!(page.content_hash(), hash);
}

#[test]
fn invalid_spans_cannot_delete_reorder_or_change_the_original_page() {
    let page = Page::from_markdown("# Original\n\nBefore\n\nKeep\n").unwrap();
    let ids = order(&page);
    let cases = [
        Span {
            start: 3,
            end: 1,
            key: "b".into(),
            value: String::new(),
        },
        Span {
            start: 2,
            end: 3,
            key: "i".into(),
            value: String::new(),
        },
        Span {
            start: 0,
            end: 5,
            key: "b".into(),
            value: String::new(),
        },
        Span {
            start: 0,
            end: 1,
            key: "unknown".into(),
            value: String::new(),
        },
        Span {
            start: 0,
            end: 1,
            key: "a".into(),
            value: "x".repeat(MAX_SPAN_VALUE_BYTES + 1),
        },
    ];
    for span in cases {
        let mut invalid = edit(&ids[0], "a😀b");
        invalid.kind = Kind::Heading {
            level: 3,
            setext: false,
        };
        invalid.spans.push(span);
        assert_refused_without_mutation(
            &page,
            &[ids[1].clone(), ids[0].clone()],
            &[edit(&ids[1], "Already changed"), invalid],
        );
    }
    let mut excessive = edit(&ids[0], "a");
    excessive.spans = vec![
        Span {
            start: 0,
            end: 1,
            key: "b".into(),
            value: String::new(),
        };
        MAX_SPANS_PER_BLOCK + 1
    ];
    assert_refused_without_mutation(&page, &ids, &[excessive]);
}

#[test]
fn invalid_block_identity_plans_are_atomic() {
    let page = fixture();
    let cases = [
        (vec!["a".into(), "a".into()], vec![edit("a", "Changed")]),
        (
            vec!["a".into(), String::new()],
            vec![edit("a", "Changed"), edit("", "New")],
        ),
        (
            vec!["a".into(), " ".into()],
            vec![edit("a", "Changed"), edit(" ", "New")],
        ),
        (vec!["a".into(), "new".into()], vec![edit("a", "Changed")]),
        (
            vec!["a".into()],
            vec![edit("a", "Changed"), edit("b", "Orphan")],
        ),
        (order(&page), vec![edit("a", "First"), edit("a", "Second")]),
    ];
    for (ids, edits) in cases {
        assert_refused_without_mutation(&page, &ids, &edits);
    }
    let long_id = "x".repeat(MAX_BLOCK_ID_BYTES + 1);
    assert_refused_without_mutation(
        &page,
        &["a".into(), long_id.clone()],
        &[edit("a", "Changed"), edit(&long_id, "New")],
    );
}

#[test]
fn an_import_with_ambiguous_block_ids_cannot_be_edited() {
    let page = Page::new();
    let list = page.blocks_list();
    for text in ["First", "Second"] {
        let mut block = Block::new(Kind::Paragraph, vec![Run::plain(text)]);
        block.id = "same".into();
        let map = list.insert_container(list.len(), LoroMap::new()).unwrap();
        write_block(&map, &block, false).unwrap();
    }
    page.doc.commit();
    assert_refused_without_mutation(&page, &["same".into()], &[edit("same", "Changed")]);
}

#[test]
fn exact_utf16_scalar_ranges_and_explicit_new_blocks_apply() {
    let page = fixture();
    let mut block = edit("new", "a😀b");
    block.spans.push(Span {
        start: 1,
        end: 3,
        key: "b".into(),
        value: String::new(),
    });
    page.apply(
        &["a".into(), "new".into(), "b".into(), "c".into()],
        &[block],
    )
    .unwrap();
    let inserted = page.blocks().into_iter().find(|b| b.id == "new").unwrap();
    assert_eq!(inserted.plain_text(), "a😀b");
    assert_eq!(
        spans_of(&inserted.runs),
        vec![Span {
            start: 1,
            end: 3,
            key: "b".into(),
            value: String::new(),
        }]
    );
}

#[test]
fn semantic_no_ops_preserve_original_markdown_and_frontiers() {
    let source = "Title\n=====\n\n*Alpha* _Bravo_\n\n```rust\nlet a = 1;\n```\n";
    let page = Page::from_markdown(source).unwrap();
    assert_eq!(page.to_markdown(), source);
    let snapshot = page.snapshot();
    let frontiers = page.frontiers();
    let before = page.blocks();
    let edits: Vec<_> = before
        .iter()
        .map(|b| BlockEdit {
            id: b.id.clone(),
            // Setext is source spelling, not a change to the visible heading.
            kind: match &b.kind {
                Kind::Heading { level, .. } => Kind::Heading {
                    level: *level,
                    setext: false,
                },
                kind => kind.clone(),
            },
            text: b.plain_text(),
            spans: spans_of(&b.runs),
        })
        .collect();
    page.apply(&order(&page), &edits).unwrap();
    assert_eq!(page.to_markdown(), source);
    assert_eq!(page.blocks(), before);
    assert_eq!(page.snapshot(), snapshot);
    assert_eq!(page.frontiers(), frontiers);
}

#[test]
fn semantic_hashes_ignore_history_source_spelling_and_run_segmentation() {
    let attrs = Attrs {
        bold: true,
        italic: true,
        ..Attrs::default()
    };
    let a = Block::new(
        Kind::Heading {
            level: 1,
            setext: true,
        },
        vec![Run {
            text: "ab".into(),
            attrs: attrs.clone(),
        }],
    );
    let mut b = a.clone();
    b.id = "different".into();
    b.kind = Kind::Heading {
        level: 1,
        setext: false,
    };
    b.src = "different Markdown".into();
    b.fp = "different importer fingerprint".into();
    b.lead = "\n\n".into();
    b.tail = "\n\n".into();
    b.runs = vec![
        Run {
            text: "a".into(),
            attrs: attrs.clone(),
        },
        Run {
            text: "b".into(),
            attrs,
        },
    ];
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(hex::decode(a.content_hash()).unwrap().len(), 32);
    b.runs[0].attrs.bold = false;
    assert_ne!(a.content_hash(), b.content_hash());

    let page = fixture();
    let hash = page.content_hash();
    let version = page.version();
    page.apply(&order(&page), &[edit("a", "Temporary")])
        .unwrap();
    page.apply(&order(&page), &[edit("a", "Alpha")]).unwrap();
    assert_ne!(page.version(), version);
    assert_eq!(page.content_hash(), hash);
    assert_eq!(page.block_hash("a"), Some(page.blocks()[0].content_hash()));
    assert!(page.block_hash("missing").is_none());
    page.apply(&["c".into(), "a".into(), "b".into()], &[])
        .unwrap();
    assert_ne!(page.content_hash(), hash);
    let moved_hash = page.content_hash();
    page.apply(
        &["c".into(), "replacement".into(), "b".into()],
        &[edit("replacement", "Alpha")],
    )
    .unwrap();
    assert_ne!(page.content_hash(), moved_hash);
}

fn merge_edit_from_observed_frontiers(remote_edit: impl FnOnce(&Page)) -> Page {
    let live = fixture();
    let observed = live.frontiers();
    let base = live.version();
    let original = live.blocks();
    let remote = live.at(&observed).unwrap();
    remote_edit(&remote);
    let remote_delta = remote.updates_since(&base);
    live.import(&remote_delta).unwrap();

    // The native editor observed the old blocks, even though live has newer work now.
    let editor = live.at(&observed).unwrap();
    assert_eq!(editor.blocks(), original);
    editor
        .apply(&order(&editor), &[edit("a", "Local Alpha")])
        .unwrap();
    let editor_delta = editor.updates_since(&base);
    live.import(&editor_delta).unwrap();
    remote.import(&editor_delta).unwrap();
    editor.import(&remote_delta).unwrap();
    assert_eq!(live.blocks(), remote.blocks());
    assert_eq!(live.blocks(), editor.blocks());
    assert_eq!(live.content_hash(), remote.content_hash());
    live
}

#[test]
fn observed_fork_edit_preserves_a_concurrent_insertion() {
    let page = merge_edit_from_observed_frontiers(|remote| {
        remote
            .apply(
                &["a".into(), "new".into(), "b".into(), "c".into()],
                &[edit("new", "Remote insertion")],
            )
            .unwrap();
    });
    assert_eq!(order(&page), ["a", "new", "b", "c"]);
    assert_eq!(page.blocks()[1].plain_text(), "Remote insertion");
    assert_eq!(page.blocks()[0].plain_text(), "Local Alpha");
}

#[test]
fn observed_fork_edit_does_not_resurrect_a_concurrent_deletion() {
    let page = merge_edit_from_observed_frontiers(|remote| {
        remote.apply(&["a".into(), "c".into()], &[]).unwrap();
    });
    assert_eq!(order(&page), ["a", "c"]);
    assert_eq!(page.blocks()[0].plain_text(), "Local Alpha");
}

#[test]
fn observed_fork_edit_does_not_reverse_a_concurrent_move() {
    let page = merge_edit_from_observed_frontiers(|remote| {
        remote
            .apply(&["c".into(), "a".into(), "b".into()], &[])
            .unwrap();
    });
    assert_eq!(order(&page), ["c", "a", "b"]);
    assert_eq!(page.blocks()[1].plain_text(), "Local Alpha");
}

#[test]
fn observed_fork_edit_merges_distinct_text_and_formatting_changes() {
    let page = merge_edit_from_observed_frontiers(|remote| {
        let mut changed = edit("b", "Remote Bravo");
        changed.spans.push(Span {
            start: 0,
            end: 6,
            key: "i".into(),
            value: String::new(),
        });
        remote.apply(&order(remote), &[changed]).unwrap();
    });
    assert_eq!(page.blocks()[0].plain_text(), "Local Alpha");
    assert_eq!(page.blocks()[1].plain_text(), "Remote Bravo");
    assert!(page.blocks()[1].runs[0].attrs.italic);
}

fn assert_import_refused_without_mutation(page: &Page, delta: &[u8]) {
    let blocks = page.blocks();
    let markdown = page.to_markdown();
    let snapshot = page.snapshot();
    let frontiers = page.frontiers();
    let version = page.version();
    let hash = page.content_hash();
    let error = page.import(delta).unwrap_err();
    assert!(error.0.contains("missing dependencies"));
    assert_eq!(page.blocks(), blocks);
    assert_eq!(page.to_markdown(), markdown);
    assert_eq!(page.snapshot(), snapshot);
    assert_eq!(page.frontiers(), frontiers);
    assert_eq!(page.version(), version);
    assert_eq!(page.content_hash(), hash);
}

#[test]
fn incomplete_import_does_not_remember_pending_changes() {
    let source = fixture();
    let base_snapshot = source.snapshot();
    let base = source.version();
    source
        .apply(&order(&source), &[edit("a", "Later Alpha")])
        .unwrap();
    let delta = source.updates_since(&base);
    let receiving = Page::new();
    assert_import_refused_without_mutation(&receiving, &delta);

    // Supplying the dependency must not silently apply the refused later edit.
    receiving.import(&base_snapshot).unwrap();
    assert_eq!(receiving.blocks()[0].plain_text(), "Alpha");
    receiving.import(&delta).unwrap();
    assert_eq!(receiving.blocks(), source.blocks());
    assert_eq!(receiving.content_hash(), source.content_hash());
}

#[test]
fn incomplete_mixed_import_refuses_even_independent_changes() {
    let source = fixture();
    let base_snapshot = source.snapshot();
    let base = source.version();
    let independent = Page::new();
    independent
        .apply(
            &["independent".into()],
            &[edit("independent", "Own history")],
        )
        .unwrap();
    source.import(&independent.snapshot()).unwrap();
    source
        .apply(&order(&source), &[edit("a", "Later Alpha")])
        .unwrap();
    let delta = source.updates_since(&base);

    // Verify this fixture really contains an applicable part and a pending part.
    let raw_receiving = configured();
    let status = raw_receiving.import(&delta).unwrap();
    assert!(!status.success.is_empty());
    assert!(status.pending.is_some_and(|pending| !pending.is_empty()));
    let receiving = Page::new();
    assert_import_refused_without_mutation(&receiving, &delta);

    receiving.import(&base_snapshot).unwrap();
    assert_eq!(order(&receiving), ["a", "b", "c"]);
    assert_eq!(receiving.blocks()[0].plain_text(), "Alpha");
    receiving.import(&delta).unwrap();
    assert_eq!(receiving.blocks(), source.blocks());
    assert_eq!(receiving.version(), source.version());
    assert_eq!(receiving.content_hash(), source.content_hash());
    let completed = receiving.snapshot();
    receiving.import(&delta).unwrap();
    assert_eq!(receiving.snapshot(), completed);
}
