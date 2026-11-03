//! Validate observed typed edits before any branch is changed.
use super::{
    codec, error, parse_prose_kind, provenance, BlockEdit, HashMap, HashSet, Kind, Page,
    PlanBlockContent, PlanBlockEdit, PlanDocument, PlanError, Result, Span, MAX_PLAN_ATOMS,
    MAX_PLAN_PACKET_BYTES,
};
pub(super) fn validate_edit(
    current: &Page,
    observed: &PlanDocument,
    order: &[String],
    changed: &[PlanBlockEdit],
) -> Result<()> {
    let blocks = observed.blocks()?;
    let existing: HashMap<_, _> = blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let ordered = crate::validate_ids(order).map_err(error)?;
    if order.len() < 2 || order[0] != blocks[0].id || order[1] != blocks[1].id {
        return Err(error("plan anchors must remain unique and first"));
    }
    let historical = provenance::historical_ids(current)?;
    let mut edited = HashSet::new();
    let mut bytes = order
        .iter()
        .try_fold(0usize, |sum, id| sum.checked_add(id.len()))
        .ok_or(PlanError::Limit("plan edit bytes"))?;
    for edit in changed {
        if !ordered.contains(edit.id.as_str()) || !edited.insert(edit.id.as_str()) {
            return Err(error("changed block must occur once in the order"));
        }
        if edit.text.chars().count() > MAX_PLAN_ATOMS
            || edit.spans.len() > crate::MAX_SPANS_PER_BLOCK
        {
            return Err(PlanError::Limit("plan edit characters or spans"));
        }
        let attributes = match &edit.content {
            PlanBlockContent::Line { legacy_id, .. } => legacy_id.len(),
            PlanBlockContent::Prose { kind } => {
                let strings = match kind {
                    Kind::Code { lang } => lang.len(),
                    Kind::Image { url, alt, title } => url
                        .len()
                        .checked_add(alt.len())
                        .and_then(|n| n.checked_add(title.len()))
                        .ok_or(PlanError::Limit("plan prose kind"))?,
                    _ => 0,
                };
                if strings > crate::MAX_SPAN_VALUE_BYTES {
                    return Err(PlanError::Limit("plan prose kind"));
                }
                strings
            }
            _ => 0,
        };
        if attributes > MAX_PLAN_PACKET_BYTES {
            return Err(PlanError::Limit("plan edit bytes"));
        }
        for span in &edit.spans {
            bytes = bytes
                .checked_add(span.key.len())
                .and_then(|n| n.checked_add(span.value.len()))
                .and_then(|n| n.checked_add(32))
                .filter(|n| *n <= MAX_PLAN_PACKET_BYTES)
                .ok_or(PlanError::Limit("plan edit bytes"))?;
        }
        bytes = bytes
            .checked_add(edit.text.len())
            .and_then(|n| {
                codec::bounded_json(&edit.content)
                    .ok()
                    .and_then(|content| n.checked_add(content.len()))
            })
            .filter(|n| *n <= MAX_PLAN_PACKET_BYTES)
            .ok_or(PlanError::Limit("plan edit bytes"))?;
        validate_spans(edit)?;
        let old = existing.get(edit.id.as_str()).copied();
        validate_content(&edit.id, &edit.content, old.map(|block| &block.content))?;
        if old.is_none() && historical.contains_key(&edit.id) {
            return Err(error("a historical plan block ID cannot be reused"));
        }
    }
    if bytes > MAX_PLAN_PACKET_BYTES || order.len() > MAX_PLAN_ATOMS {
        return Err(PlanError::Limit("plan edit size"));
    }
    if order
        .iter()
        .any(|id| !existing.contains_key(id.as_str()) && !edited.contains(id.as_str()))
    {
        return Err(error("new plan block requires typed content"));
    }
    Ok(())
}

fn validate_content(
    id: &str,
    target: &PlanBlockContent,
    old: Option<&PlanBlockContent>,
) -> Result<()> {
    if old.is_some_and(|old| old.role() != target.role())
        || (old.is_none()
            && matches!(
                target,
                PlanBlockContent::Title { .. } | PlanBlockContent::Summary
            ))
    {
        return Err(error("plan block roles are immutable"));
    }
    match target {
        PlanBlockContent::Title { budget_cents } => {
            if budget_cents.is_some_and(|cents| cents < 0) && old != Some(target) {
                return Err(error("new budget cannot be negative"));
            }
        }
        PlanBlockContent::Line {
            legacy_id,
            cost_cents,
            ..
        } => {
            let previous = match old {
                Some(PlanBlockContent::Line {
                    legacy_id: prior_id,
                    cost_cents,
                    ..
                }) => {
                    if prior_id != legacy_id {
                        return Err(error("legacy line identity is immutable"));
                    }
                    Some(*cost_cents)
                }
                None => {
                    if *legacy_id != format!("ln_{id}") {
                        return Err(error("new line identity must derive from its block ID"));
                    }
                    None
                }
                _ => return Err(error("plan block roles are immutable")),
            };
            if *cost_cents < 0 && previous != Some(*cost_cents) {
                return Err(error("new cost cannot be negative"));
            }
        }
        PlanBlockContent::Prose { kind } => {
            parse_prose_kind(&serde_json::to_string(kind).map_err(error)?)?;
        }
        PlanBlockContent::Summary | PlanBlockContent::Section => {}
    }
    Ok(())
}

pub(super) fn validate_spans(edit: &PlanBlockEdit) -> Result<Vec<Span>> {
    crate::validate_spans(&BlockEdit {
        id: edit.id.clone(),
        kind: Kind::Paragraph,
        text: edit.text.clone(),
        spans: edit.spans.clone(),
    })
    .map_err(error)?;
    let mut ranges: HashMap<&str, Vec<Span>> = HashMap::new();
    for span in &edit.spans {
        if span.key != "a" && !matches!(span.value.as_str(), "" | "1") {
            return Err(error("boolean plan marks require an empty value or one"));
        }
        if span.start < span.end {
            let mut normalized = span.clone();
            if normalized.key != "a" {
                normalized.value.clear();
            }
            ranges.entry(&span.key).or_default().push(normalized);
        }
    }
    let mut merged: Vec<Span> = Vec::new();
    for group in ranges.values_mut() {
        group.sort_by_key(|span| (span.start, span.end));
        for span in group.iter() {
            if let Some(prior) = merged.last_mut().filter(|prior| prior.key == span.key) {
                if span.start < prior.end && span.value != prior.value {
                    return Err(error("conflicting overlapping plan marks"));
                }
                if span.start <= prior.end && span.value == prior.value {
                    prior.end = prior.end.max(span.end);
                    continue;
                }
            }
            merged.push(span.clone());
        }
    }
    Ok(merged)
}
