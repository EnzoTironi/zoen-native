//! Export an observed historical document, never the private live draft.
use super::{doc_err, invalid, t, CoreError, Engine, ItemContent, R};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct PageVersionExport {
    pub item_id: String,
    pub space_id: String,
    pub version: u32,
    pub title: String,
    pub content_hash: String,
    pub markdown: String,
}

impl Engine {
    pub(crate) fn page_export_version(
        &self,
        space: &str,
        item: &str,
        version: u32,
        observed_hash: &str,
    ) -> R<PageVersionExport> {
        if !self.is_member(space, &self.me_id()?) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros exportam esta página",
                    "only members can export this page",
                ),
            });
        }
        let it = self.item_state(item)?;
        if it.space != space || self.state.is_recovery_item(item) {
            return Err(crate::CoreError::NotFound {
                what: "Item".into(),
            });
        }
        if self.space_state(space)?.integrity_error.is_some() {
            return Err(invalid(
                "o histórico precisa ser recuperado",
                "history needs recovery",
            ));
        }
        self.with_page(item, |it, session| {
            // Refresh authenticated history, retaining the existing private journal.
            // An older available frontier can still be exported if a later blob
            // is downloading. binary_at refuses an unavailable selected frontier.
            session.read(self)?;
            let page = session.binary_at(version)?;
            let content_hash = page.content_hash().map_err(doc_err)?;
            if content_hash != observed_hash {
                return Err(invalid(
                    "esta versão mudou; abra a prévia novamente",
                    "this version changed; open its preview again",
                ));
            }
            let content = version
                .checked_sub(1)
                .and_then(|index| it.versions.get(index as usize))
                .and_then(|v| match &v.content {
                    ItemContent::Page(content) => Some(content),
                    _ => None,
                })
                .ok_or_else(|| invalid("versão não encontrada", "version not found"))?;
            Ok(PageVersionExport {
                item_id: item.into(),
                space_id: space.into(),
                version,
                title: content.title.clone(),
                content_hash,
                markdown: page.to_markdown().map_err(doc_err)?,
            })
        })
    }
}
