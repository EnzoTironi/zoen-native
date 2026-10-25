//! Public Markdown bytes that failed the PR222 two-device corpus journey.
//! This isolates document parsing and snapshot transfer from relay/network behavior.
use documents::Page;

fn exact_roundtrip(source: &str) {
    let page = Page::from_markdown(source).unwrap();
    assert_eq!(page.to_markdown().unwrap().as_bytes(), source.as_bytes());
    let reopened = Page::new();
    reopened.import(&page.snapshot()).unwrap();
    assert_eq!(
        reopened.to_markdown().unwrap().as_bytes(),
        source.as_bytes()
    );
}

#[test]
fn reference_screens_keep_every_markdown_byte() {
    exact_roundtrip(include_str!("../../../docs/telas-referencia.md"));
}

#[test]
fn motion_reference_keeps_every_markdown_byte() {
    exact_roundtrip(include_str!("../../../docs/ux-motion.md"));
}
