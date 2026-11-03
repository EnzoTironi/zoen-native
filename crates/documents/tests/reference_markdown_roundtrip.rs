//! Public Markdown bytes that failed the PR222 two-device corpus journey.
//! This isolates document parsing and snapshot transfer from relay/network behavior.
use documents::{DocError, Page};

fn exact_roundtrip(source: &str) -> Result<(), DocError> {
    let page = Page::from_markdown(source)?;
    assert_eq!(page.to_markdown()?.as_bytes(), source.as_bytes());
    let reopened = Page::new();
    reopened.import(&page.snapshot())?;
    assert_eq!(reopened.to_markdown()?.as_bytes(), source.as_bytes());
    Ok(())
}

#[test]
fn reference_screens_keep_every_markdown_byte() -> Result<(), DocError> {
    exact_roundtrip(include_str!("../../../docs/telas-referencia.md"))
}

#[test]
fn motion_reference_keeps_every_markdown_byte() -> Result<(), DocError> {
    exact_roundtrip(include_str!("../../../docs/ux-motion.md"))
}
