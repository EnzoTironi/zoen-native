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
        std::slice::from_ref(&block.id),
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
    assert_eq!(
        block.content_hash().unwrap(),
        "468d07970af88d64848b995d2861dae508d565709562873794e2adbccac897e3"
    );
    assert_eq!(
        page.content_hash().unwrap(),
        "3e75caf82299c5b0d6ac8c0a45733fa07bace87d9ee8923e3b41a72d0a2769d5"
    );
    assert_eq!(
        block.fingerprint().unwrap(),
        r#"{"k":{"k":"heading","level":2,"setext":true},"h":"f07fdb20e3dfee8969ce8199"}"#
    );
    assert_eq!(
        page.block_hash(&block.id).unwrap(),
        Some(block.content_hash().unwrap())
    );
}
