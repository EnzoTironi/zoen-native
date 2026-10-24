//! Authoring on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(super) fn page_write_author(&self, item: &str) -> R<String> {
        let me = self.me_id()?;
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if !self.page_writer(&it.space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só membros editam páginas", "only members can edit pages"),
            });
        }
        self.author_for(&me)?;
        Ok(me)
    }

    pub(super) fn page_writer(&self, space: &str, actor: &str) -> bool {
        self.state.spaces.get(space).is_some_and(|s| {
            s.members
                .iter()
                .any(|(id, role)| id == actor && *role != Role::Reader)
        })
    }

    pub(super) fn page_content(
        &mut self,
        space: &str,
        title: String,
        path: &str,
        bytes: Vec<u8>,
    ) -> R<PageContent> {
        let (loro, blob) = if bytes.len() > INLINE_MAX {
            (String::new(), Some(self.page_blob(space, &bytes)?))
        } else {
            (
                base64::engine::general_purpose::STANDARD.encode(&bytes),
                None,
            )
        };
        Ok(PageContent {
            title,
            path: path.to_string(),
            loro,
            blob,
            manifest: None,
        })
    }

    pub(super) fn new_page(
        &mut self,
        space: &str,
        path: &str,
        page: Page,
        origin: &str,
    ) -> R<String> {
        let me = self.me_id()?;
        if !self.page_writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só membros criam páginas", "only members can create pages"),
            });
        }
        let mut title = page.title();
        if title.is_empty() {
            title = path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".md")
                .to_string();
        }
        if title.is_empty() {
            title = t("Sem título", "Untitled");
        }
        let content = self.page_content(space, title, path, page.snapshot())?;
        let item = new_id("it")?;
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::Page,
                content: ItemContent::Page(content),
                origin: origin.into(),
            },
        )?;
        self.index_dirty = true;
        Ok(item)
    }

    /// A page made from a Markdown file; exporting it untouched gives the same bytes back.
    pub fn page_import_markdown(&mut self, space: &str, path: &str, markdown: &str) -> R<String> {
        let page = Page::from_markdown(markdown).map_err(doc_err)?;
        self.new_page(space, path, page, &t("Importado", "Imported"))
    }

    /// An empty page with a title.
    pub fn page_create(&mut self, space: &str, title: &str) -> R<String> {
        let md = if title.trim().is_empty() {
            String::new()
        } else {
            format!("# {}\n", title.trim())
        };
        let page = Page::from_markdown(&md).map_err(doc_err)?;
        let order: Vec<String> = page.blocks().iter().map(|b| b.id.clone()).collect();
        // New pages have no source to preserve: drop it so the first edit isn't special.
        let fresh = Page::new();
        let mut edits = Vec::new();
        for b in page.blocks() {
            edits.push(BlockEdit {
                id: b.id.clone(),
                kind: b.kind.clone(),
                text: b.plain_text(),
                spans: vec![],
            });
        }
        let mut order = order;
        let extra = format!("{:016x}", rand_u64());
        order.push(extra.clone());
        edits.push(BlockEdit {
            id: extra,
            kind: Kind::Paragraph,
            text: String::new(),
            spans: vec![],
        });
        fresh.apply(&order, &edits).map_err(doc_err)?;
        self.new_page(space, "", fresh, &t("Criado", "Created"))
    }
}
