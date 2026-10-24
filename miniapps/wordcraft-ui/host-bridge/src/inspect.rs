//! One full legacy text plus bounded rich detail, under the unchanged wire envelope.
use crate::{Result, failure};
use serde::Serialize;
use serde_json::{Value, json};
use wordcraft_doc::{Block, Document, StoryRef};
const MAX_METADATA: usize = 64 * 1024;
#[derive(Serialize)]
struct Envelope<'a> {
    ok: bool,
    metadata: &'a Value,
}
fn fits(metadata: &Value) -> bool {
    serde_json::to_vec(&Envelope { ok: true, metadata })
        .is_ok_and(|bytes| bytes.len() <= MAX_METADATA)
}
pub(crate) fn document(mut value: Value, doc: &Document) -> Result<Value> {
    value["text"] = json!(doc.plain_text(StoryRef::Body));
    let mut offset = Some(0usize);
    let mut paragraphs = Vec::new();
    let mut runs = 0usize;
    for (index, block) in doc.body.iter().enumerate() {
        let Block::Para(p) = block.as_ref() else {
            offset = None;
            continue;
        };
        runs += p.runs.len();
        let mut detail = json!({"paragraph":index,"runs":p.runs,"properties":p.props,"inlineObjects":p.objects.len()});
        if let Some(start) = offset.filter(|_| p.objects.is_empty()) {
            // Exactly the existing upstream story text, so ordinary text is never duplicated.
            detail["textRange"] = json!({"start":start,"end":start+p.text.len()});
        } else {
            // The stored offsets include object placeholders; flattened text cannot map them.
            detail["text"] = json!(p.text);
        }
        offset = offset.and_then(|n| n.checked_add(p.plain_text().len() + 1));
        paragraphs.push(detail);
    }
    value["richDetails"] = json!({"omitted":false,"bodyParagraphs":paragraphs.len(),"bodyRuns":runs,"styles":doc.styles.styles.len(),"offsetUnits":"utf8"});
    value["paragraphs"] = json!(paragraphs);
    value["styles"] = json!(doc.styles);
    if !fits(&value) {
        let object = value
            .as_object_mut()
            .ok_or_else(|| failure("inspection_metadata_limit"))?;
        object.remove("paragraphs");
        object.remove("styles");
        value["richDetails"]["omitted"] = json!(true);
        value["richDetails"]["reason"] = json!("metadata_limit");
    }
    if !fits(&value) {
        // Escaping alone can exceed the wire cap. Refuse explicitly, never truncate text.
        return Err(failure("inspection_metadata_limit"));
    }
    Ok(value)
}
