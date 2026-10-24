//! Real canonical edits, encrypted receipts, cold reopen and Markdown export.
use super::*;

const SOURCE: &str = "Setext **title**\n===\n\n## ATX\n\nA **bold** 👩‍💻 paragraph\n\n- Bullet\n\n0. Numbered\n\n- [x] Task\n\n> Quote\n\n```rust\nlet x = 1;\n```\n\n***\n\n![ALT](zoen:item/private-file \"Keep title\")\n\n<div>raw</div>\n";

fn imported(client: &Client, space: &str) -> PageDto {
    let item = client
        .page_import_markdown(space.into(), "lossless.md".into(), SOURCE.into())
        .unwrap();
    client.page(item.id).unwrap()
}

fn duplicated(original: &PageDto) -> Vec<PageBlockDto> {
    original
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(i, block)| {
            let mut copy = block.clone();
            copy.id = format!("copy_{i}");
            [block.clone(), copy]
        })
        .collect()
}

#[test]
fn every_kind_duplicates_undoes_replays_exports_and_recovers_cold(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(new_id("lossless-page")?);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory
        .join("client.sqlite")
        .to_string_lossy()
        .into_owned();
    let (client, seed) = fixture_at(&path);
    let original = imported(&client, &seed.space_id);
    let mut kinds: Vec<_> = original.blocks.iter().map(|b| b.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        [
            "bullet",
            "code",
            "divider",
            "heading",
            "image",
            "numbered",
            "paragraph",
            "quote",
            "raw",
            "task"
        ]
    );
    let copies = duplicated(&original);
    let accepted = apply(&client, &original, "duplicate-all", &copies).unwrap();
    assert_eq!(accepted.page.blocks, copies);
    assert_ne!(accepted.page.content_hash, original.content_hash);
    for pair in accepted.page.blocks.as_chunks::<2>().0 {
        let mut normalized = pair[1].clone();
        normalized.id = pair[0].id.clone();
        assert_eq!(normalized, pair[0]);
    }
    let rows = journal_rows(&client);
    let replay = apply(&client, &original, "duplicate-all", &copies).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(journal_rows(&client), rows);
    let mut conflicting = copies.clone();
    conflicting
        .iter_mut()
        .find(|b| b.kind == "image" && b.id.starts_with("copy_"))
        .unwrap()
        .title = Some("not the accepted title".into());
    assert!(apply(&client, &original, "duplicate-all", &conflicting).is_err());
    assert_eq!(
        client.page(original.item_id.clone()).unwrap().blocks,
        copies
    );
    let mut own = accepted.page.clone();
    own.edit_context = accepted.applied_edit_context;
    let undone = apply(&client, &own, "undo-copies", &original.blocks).unwrap();
    assert_eq!(undone.page.blocks, original.blocks);
    let redone = apply(&client, &undone.page, "redo-copies", &copies).unwrap();
    assert_eq!(redone.page.blocks, copies);
    let markdown = client.page_markdown(original.item_id.clone()).unwrap();
    assert_eq!(markdown.matches("0. Numbered").count(), 2);
    assert_eq!(markdown.matches("Setext **title**\n===").count(), 2);
    assert_eq!(
        markdown
            .matches("![ALT](zoen:item/private-file \"Keep title\")")
            .count(),
        2
    );
    let exported = documents::Page::from_markdown(&markdown).unwrap();
    assert_eq!(
        exported
            .blocks()
            .iter()
            .filter(|b| matches!(b.kind, Kind::Heading { setext: true, .. }))
            .count(),
        2
    );
    assert_eq!(
        exported
            .blocks()
            .iter()
            .filter(|b| matches!(&b.kind, Kind::Image { title, .. } if title == "Keep title"))
            .count(),
        2
    );
    assert!(client
        .page_commit(original.item_id.clone(), "Duplicated".into())
        .unwrap());
    let shared = client.page(original.item_id.clone()).unwrap();
    drop(client);
    let client = Client::open(path, "en".into()).unwrap();
    assert_eq!(
        client.page(original.item_id.clone()).unwrap().blocks,
        copies
    );
    assert_eq!(
        client.page_markdown(original.item_id.clone()).unwrap(),
        markdown
    );
    let export = client
        .lock()
        .page_export_version(
            &shared.space_id,
            &shared.item_id,
            shared.version,
            &shared.content_hash,
        )
        .unwrap();
    assert_eq!(export.markdown, markdown);
    let replay = apply(&client, &original, "duplicate-all", &copies).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(replay.page.blocks, copies);
    drop(client);
    std::fs::remove_dir_all(directory).unwrap();
    Ok(())
}

#[test]
fn absent_legacy_metadata_preserves_content_and_exact_old_receipt_fingerprint(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    // Original field order/types, deliberately independent of the extended DTO serializer.
    #[derive(serde::Serialize)]
    struct OldBlock<'a> {
        id: &'a str,
        kind: &'a str,
        level: u32,
        indent: u32,
        number: u32,
        checked: bool,
        lang: &'a str,
        url: &'a str,
        alt: &'a str,
        text: &'a str,
        spans: &'a [TextSpanDto],
    }
    let directory = std::env::temp_dir().join(new_id("legacy-page-fields")?);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory
        .join("client.sqlite")
        .to_string_lossy()
        .into_owned();
    let (client, seed) = fixture_at(&path);
    let original = imported(&client, &seed.space_id);
    let mut changed = original.blocks.clone();
    for block in &mut changed {
        block.setext = None;
        block.title = None;
        block.text.push('!');
    }
    let order: Vec<_> = changed.iter().map(|b| b.id.clone()).collect();
    let legacy: Vec<_> = changed
        .iter()
        .map(|b| OldBlock {
            id: &b.id,
            kind: &b.kind,
            level: b.level,
            indent: b.indent,
            number: b.number,
            checked: b.checked,
            lang: &b.lang,
            url: &b.url,
            alt: &b.alt,
            text: &b.text,
            spans: &b.spans,
        })
        .collect();
    let decoded: Vec<PageBlockDto> =
        serde_json::from_slice(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(
        decoded, changed,
        "old JSON without metadata decodes identically"
    );
    let bytes =
        serde_json::to_vec(&("legacy-edit", &original.edit_context, &order, legacy)).unwrap();
    let hash = hex::encode(
        Sha256::new()
            .chain_update(b"zoen.page.edit.request.v1\0")
            .chain_update(bytes)
            .finalize(),
    );
    let accepted = apply(&client, &original, "legacy-edit", &changed).unwrap();
    assert_eq!(accepted.mutation_hash, hash);
    assert_eq!(accepted.page.blocks[0].setext, Some(true));
    assert_eq!(
        accepted
            .page
            .blocks
            .iter()
            .find(|b| b.kind == "image")
            .unwrap()
            .title
            .as_deref(),
        Some("Keep title")
    );
    drop(client);
    let client = Client::open(path, "en".into()).unwrap();
    let replay = apply(&client, &original, "legacy-edit", &changed).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.mutation_hash, hash);
    assert_eq!(replay.page.blocks, accepted.page.blocks);
    let mut explicit = accepted.page.blocks.clone();
    explicit[0].setext = Some(false);
    explicit
        .iter_mut()
        .find(|b| b.kind == "image")
        .unwrap()
        .title = Some(String::new());
    let result = apply(&client, &accepted.page, "explicit-clear", &explicit).unwrap();
    assert_eq!(result.page.blocks, explicit);
    assert!(!client
        .page_markdown(original.item_id)
        .unwrap()
        .contains("Keep title"));
    assert_eq!(
        result
            .page
            .blocks
            .iter()
            .find(|b| b.kind == "numbered")
            .unwrap()
            .number,
        0
    );
    drop(client);
    std::fs::remove_dir_all(directory).unwrap();
    Ok(())
}

#[test]
fn future_kinds_and_malformed_heading_metadata_cannot_mutate_or_write_receipts() {
    let (client, original) = fixture();
    let rows = journal_rows(&client);
    for (kind, level, setext) in [
        ("future", 1, Some(false)),
        ("heading", 0, Some(false)),
        ("heading", 7, Some(false)),
        ("heading", 3, Some(true)),
    ] {
        let mut bad = original.blocks[0].clone();
        bad.kind = kind.into();
        bad.level = level;
        bad.setext = setext;
        assert!(apply(&client, &original, "refused", &[bad]).is_err());
        assert_eq!(
            client.page(original.item_id.clone()).unwrap().blocks,
            original.blocks
        );
        assert_eq!(journal_rows(&client), rows);
    }
    let mut json = serde_json::to_value(&original.blocks[0]).unwrap();
    for field in ["setext", "title"] {
        json[field] = serde_json::json!(["wrong type"]);
        assert!(serde_json::from_value::<PageBlockDto>(json.clone()).is_err());
        json.as_object_mut().unwrap().remove(field);
    }
}

#[test]
fn syntax_only_edit_has_durable_undo_and_version_without_changing_semantic_hash() {
    let (client, seed) = fixture();
    let original = imported(&client, &seed.space_id);
    let mut explicit = original.blocks.clone();
    explicit[0].setext = Some(false);
    let accepted = apply(&client, &original, "syntax-only", &explicit).unwrap();
    assert_eq!(accepted.page.content_hash, original.content_hash);
    assert!(accepted.page.unsaved);
    assert_eq!(accepted.page.blocks, explicit);
    assert!(client
        .page_markdown(original.item_id.clone())
        .unwrap()
        .starts_with("# Setext"));
    client.lock().reload().unwrap();
    let reloaded = client.page(original.item_id.clone()).unwrap();
    assert!(reloaded.unsaved);
    assert_eq!(reloaded.blocks, explicit);
    let undo = apply(&client, &reloaded, "undo-syntax", &original.blocks).unwrap();
    assert_eq!(undo.page.blocks, original.blocks);
    let redo = apply(&client, &undo.page, "redo-syntax", &explicit).unwrap();
    assert!(redo.page.unsaved);
    assert!(client
        .page_commit(original.item_id.clone(), "Syntax".into())
        .unwrap());
    client.lock().reload().unwrap();
    assert_eq!(client.page(original.item_id).unwrap().blocks, explicit);
}
