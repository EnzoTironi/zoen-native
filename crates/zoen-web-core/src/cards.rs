//! Space-local item projection for timeline cards. Never consult fixture metadata.
use roda_types::{Event, EventBody, ItemContent, ItemKind};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCard {
    pub item_id: String,
    pub title: String,
    pub summary: String,
    pub kind_label: &'static str,
    pub kind_id: &'static str,
    pub version: u32,
    pub total_cents: Option<i64>,
    pub budget_cents: Option<i64>,
    pub line_count: u32,
    pub done_count: u32,
    pub snapshot: Option<serde_json::Value>,
}

pub(crate) fn project(events: &[Event], now_ms: i64, english: bool) -> BTreeMap<String, ItemCard> {
    let mut items: BTreeMap<String, (ItemKind, Vec<ItemContent>)> = BTreeMap::new();
    for event in events {
        match &event.body {
            EventBody::ItemCreated {
                item,
                kind,
                content,
                ..
            } => {
                items
                    .entry(item.clone())
                    .or_insert_with(|| (*kind, vec![content.clone()]));
            }
            EventBody::ItemVersioned { item, content, .. } => {
                if let Some((_, versions)) = items.get_mut(item) {
                    versions.push(content.clone());
                }
            }
            EventBody::ItemReverted {
                item, to_version, ..
            } => {
                if let Some((_, versions)) = items.get_mut(item) {
                    if let Some(content) = to_version
                        .checked_sub(1)
                        .and_then(|i| versions.get(i as usize))
                        .cloned()
                    {
                        versions.push(content);
                    }
                }
            }
            _ => {}
        }
    }
    items
        .into_iter()
        .filter_map(|(item, (kind, versions))| {
            let content = versions.last()?;
            if matches!(content, ItemContent::File(_)) {
                return None;
            }
            let (kind_id, kind_label) = match kind {
                ItemKind::Plan => ("plan", "Plan"),
                ItemKind::Task => ("task", "Task"),
                ItemKind::Note => ("note", "Note"),
                ItemKind::App => ("app", "Mini-app"),
                ItemKind::Page => ("page", "Page"),
                ItemKind::File => ("file", "File"),
            };
            let mut card = ItemCard {
                item_id: item.clone(),
                title: content.title(),
                summary: String::new(),
                kind_label,
                kind_id,
                version: versions.len() as u32,
                total_cents: None,
                budget_cents: None,
                line_count: 0,
                done_count: 0,
                snapshot: None,
            };
            match content {
                ItemContent::Plan(plan) => {
                    card.summary = plan.summary.clone();
                    card.budget_cents = plan.budget_cents;
                    let lines: Vec<_> = plan.sections.iter().flat_map(|s| &s.lines).collect();
                    card.total_cents = lines
                        .iter()
                        .try_fold(0i64, |total, line| total.checked_add(line.cost_cents));
                    card.line_count = lines.len() as u32;
                    card.done_count = lines.iter().filter(|l| l.done).count() as u32;
                }
                ItemContent::App(app) => {
                    card.snapshot = serde_json::from_str(&app.state_json)
                        .ok()
                        .and_then(|state| {
                            roda_types::widget::snapshot(&app.app, &state, &item, now_ms, english)
                        });
                }
                ItemContent::Text { text } => card.summary = text.clone(),
                // Page body is a Loro document, not plain text. Do not invent a summary.
                _ => {}
            }
            Some((item, card))
        })
        .collect()
}
