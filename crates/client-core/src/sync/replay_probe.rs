//! Test-only work observation. No production state, timer or authority.
use std::cell::Cell;

thread_local! {
    static FOLDED: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn folded() {
    FOLDED.set(FOLDED.get() + 1);
}

pub(crate) fn reset() {
    FOLDED.set(0);
}

pub(crate) fn steps() -> usize {
    FOLDED.get()
}

use crate::engine::Engine;
use base64::Engine as _;
use types::{EventBody, ItemContent, ItemKind, PageContent};

pub(crate) fn page_body(index: usize) -> EventBody {
    let markdown = format!("# Page {index}\n\nUnicode 😀 and **unchanged** source.\n");
    let page = documents::Page::from_markdown(&markdown).unwrap();
    EventBody::ItemCreated {
        item: format!("remote-item-{index}"),
        kind: ItemKind::Page,
        content: ItemContent::Page(PageContent {
            title: format!("Page {index}"),
            path: format!("page-{index}.md"),
            loro: base64::engine::general_purpose::STANDARD.encode(page.snapshot()),
            blob: None,
            manifest: None,
        }),
        origin: "Imported".into(),
    }
}

pub(crate) fn projection(engine: &Engine) -> std::collections::BTreeMap<String, String> {
    engine
        .state
        .items
        .iter()
        .map(|(id, state)| (id.clone(), format!("{state:?}")))
        .collect()
}

pub(crate) struct Home(pub(crate) std::path::PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
