//! Closed manual editing profile; canonical authorization stays in the host.
use crate::profile;
use serde_json::Value;
use wordcraft_engine::{CmdError, Session};
fn denied() -> CmdError {
    CmdError::Disabled("This action is unavailable in this document.".into())
}
/// # Errors
/// Refuses commands outside the declared plain document editor or a frozen input lease.
pub fn command_policy(s: &Session, id: &str, params: &Value) -> Result<(), CmdError> {
    let read = matches!(id, "document.inspect" | "edit.undoHistory");
    if s.managed_frozen && !read {
        return Err(denied());
    }
    if id.starts_with("caret.") || id.starts_with("select.") || read {
        return Ok(());
    }
    match id {
        "text.insert" | "edit.paste" | "edit.pasteText" => {
            let object = params.as_object().ok_or_else(denied)?;
            if object.keys().any(|k| !matches!(k.as_str(), "text" | "raw")) {
                return Err(denied());
            }
            if let Some(text) = params.get("text") {
                let text = text.as_str().ok_or_else(denied)?;
                profile::validate_text(text).map_err(|_| denied())?;
                let current = s.doc.plain_text(wordcraft_doc::StoryRef::Body).len();
                // Deletion/Undo remain available if an upstream generated operation exceeded
                // the profile. A save still validates the entire actual model and writer.
                if current
                    .checked_add(text.len())
                    .is_none_or(|n| n > profile::MAX_TEXT)
                {
                    return Err(denied());
                }
            }
            if params.get("raw").is_some_and(|v| !v.is_boolean()) {
                return Err(denied());
            }
        }
        "text.tab" => {
            if s.sel.anchor.story != wordcraft_doc::StoryRef::Body
                || s.sel.focus.story != wordcraft_doc::StoryRef::Body
                || s.sel.anchor.path != s.sel.focus.path
            {
                // Upstream Tab on multiple paragraphs applies indentation, outside this profile.
                return Err(denied());
            }
        }
        "text.newParagraph"
        | "text.backspace"
        | "text.delete"
        | "text.deleteWordBack"
        | "text.deleteWordForward"
        | "edit.undo"
        | "edit.redo"
        | "edit.copy"
        | "edit.cut"
        | "view.zoom"
        | "view.zoomIn"
        | "view.zoomOut"
        | "view.zoom100"
        | "view.pageWidth" => {}
        _ => return Err(denied()),
    }
    Ok(())
}
