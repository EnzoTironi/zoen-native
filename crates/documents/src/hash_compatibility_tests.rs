use crate::{Attrs, Block, BlockEdit, Kind, Page, Run, Span};

#[test]
fn visible_hash_and_source_fingerprint_legacy_vector() {
    let text = "Olá 🌥\n\"quoted\"";
    let block = Block {
        id: "stable-block".into(),
        kind: Kind::Heading {
            level: 2,
            setext: true,
        },
        runs: vec![Run {
            text: text.into(),
            attrs: Attrs {
                bold: true,
                ..Attrs::default()
            },
        }],
        src: String::new(),
        tail: String::new(),
        lead: String::new(),
        fp: String::new(),
    };
    let page = Page::new();
    page.apply(
        &[block.id.clone()],
        &[BlockEdit {
            id: block.id.clone(),
            kind: block.kind.clone(),
            text: text.into(),
            spans: vec![Span {
                start: 0,
                end: u32::try_from(text.encode_utf16().count()).unwrap(),
                key: "b".into(),
                value: "1".into(),
            }],
        }],
    )
    .unwrap();
    println!(
        "block={} page={} fingerprint={}",
        block.content_hash(),
        page.content_hash(),
        block.fingerprint()
    );
    assert_eq!(page.block_hash(&block.id), Some(block.content_hash()));
}
