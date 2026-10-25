//! A whole Page uses its observed canonical snapshot and the existing input journal.
use super::{
    invalid, tagged, text, validate_target, Binding, Capture, PageBlockDto, PageInputMode,
    PageInputTarget, MAX_CAPTURE, R,
};
use std::collections::{HashMap, HashSet, VecDeque};

pub(in crate::pages) fn validate(blocks: &[PageBlockDto]) -> R<()> {
    tagged::bounded_json(&blocks, MAX_CAPTURE)?;
    let mut ids = HashSet::new();
    for block in blocks {
        validate_target(&PageInputTarget::Block {
            block_id: block.id.clone(),
        })?;
        if !ids.insert(&block.id) {
            return Err(invalid("duplicate whole-document base block"));
        }
        super::super::kind_from_dto(block, None)?;
    }
    Ok(())
}

pub(in crate::pages) fn seed(blocks: &[PageBlockDto]) -> R<String> {
    validate(blocks)?;
    let markdown = super::super::editor_blocks_markdown(blocks)?;
    if markdown.len() > MAX_CAPTURE {
        return Err(invalid("whole-document input exceeds 1 MiB"));
    }
    Ok(markdown)
}

pub(in crate::pages) fn edits(binding: &Binding, capture: &Capture) -> R<Vec<PageBlockDto>> {
    if binding.mode != PageInputMode::Markdown {
        return Err(invalid("whole-document input requires Markdown"));
    }
    let base = binding
        .base_document
        .as_deref()
        .ok_or_else(|| invalid("missing whole-document base"))?;
    let markdown = text(&binding.mode, &capture.payload)?.text;
    if markdown == seed(base)? {
        // An untouched source view must preserve every canonical property, even
        // properties that Markdown cannot express and duplicate-looking blocks.
        return Ok(base.to_vec());
    }
    let mut originals: HashMap<String, VecDeque<&PageBlockDto>> = HashMap::new();
    for block in base {
        originals
            .entry(super::super::editor_block_markdown(block)?)
            .or_default()
            .push_back(block);
    }
    let mut changed = super::super::parse_editor_markdown(&markdown)?;
    for block in &mut changed {
        let key = super::super::editor_block_markdown(block)?;
        if let Some(original) = originals.get_mut(&key).and_then(VecDeque::pop_front) {
            // Exact unchanged representations keep their original ID, spans and
            // fixed metadata. Repeated equal blocks are consumed in source order.
            *block = original.clone();
        }
    }
    Ok(changed)
}
