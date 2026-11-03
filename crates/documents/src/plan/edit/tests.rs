use super::*;
use crate::plan::{
    find, CanonicalPlanSeed, ContainerTrait, ExpectedLine, LegacyVersion, LinePatch, PlanCommand,
    PlanDoc, PlanSection, TotalCents,
};
use loro::{IdSpan, JsonSchema, JsonTextOp};

const RAW: &str = "a😀b\r\n\0e\u{301}\r\u{85}\u{2029}tail";

fn original() -> PlanDoc {
    PlanDoc {
        title: "Title\r\nexact".into(),
        summary: "Summary\0\u{85}".into(),
        budget_cents: Some(1_000),
        sections: vec![PlanSection {
            title: "Section\r".into(),
            lines: vec![
                PlanLine {
                    id: "duplicate-legacy-".repeat(100),
                    text: RAW.into(),
                    cost_cents: 100,
                    done: false,
                },
                PlanLine {
                    id: "duplicate-legacy-".repeat(100),
                    text: "Second".into(),
                    cost_cents: 200,
                    done: false,
                },
            ],
        }],
    }
}

fn seed_with(document: PlanDoc) -> CanonicalPlanSeed {
    CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "authenticated-plan-creation".into(),
        document,
    }])
    .unwrap()
}

fn seed() -> CanonicalPlanSeed {
    seed_with(original())
}

fn order(doc: &PlanDocument) -> Vec<String> {
    doc.blocks()
        .unwrap()
        .into_iter()
        .map(|block| block.id)
        .collect()
}

fn line_block(doc: &PlanDocument) -> PlanBlock {
    doc.blocks()
        .unwrap()
        .into_iter()
        .find(|block| matches!(block.content, PlanBlockContent::Line { .. }))
        .unwrap()
}

fn edit(block: &PlanBlock) -> PlanBlockEdit {
    PlanBlockEdit {
        id: block.id.clone(),
        content: block.content.clone(),
        text: block.plain_text(),
        spans: crate::spans_of(&block.runs),
    }
}

fn span(text: &str, key: &str) -> Span {
    Span {
        start: 0,
        end: u32::try_from(text.encode_utf16().count()).unwrap(),
        key: key.into(),
        value: if key == "a" {
            "https://example.test/exact?q=é".into()
        } else {
            String::new()
        },
    }
}

fn prose(id: &str, kind: Kind, text: &str) -> PlanBlockEdit {
    PlanBlockEdit {
        id: id.into(),
        content: PlanBlockContent::Prose { kind },
        text: text.into(),
        spans: vec![],
    }
}

fn new_line(id: &str, text: &str, cents: i64) -> PlanBlockEdit {
    PlanBlockEdit {
        id: id.into(),
        content: PlanBlockContent::Line {
            legacy_id: format!("ln_{id}"),
            cost_cents: cents,
            done: false,
        },
        text: text.into(),
        spans: vec![],
    }
}

fn apply(doc: &mut PlanDocument, changed: &[PlanBlockEdit]) {
    let revision = doc.revision().unwrap();
    doc.apply_from(&revision, &order(doc), changed).unwrap();
}

fn wire(update: &PlanUpdate) -> PlanUpdate {
    PlanUpdate::from_json(&update.to_json().unwrap()).unwrap()
}

fn merge_both(seed: &CanonicalPlanSeed, branches: &[&PlanDocument]) -> PlanDocument {
    let from = seed.open_editor().unwrap().version();
    let updates: Vec<_> = branches
        .iter()
        .map(|branch| wire(&branch.updates_since(&from).unwrap()))
        .collect();
    let mut forward = seed.open_editor().unwrap();
    let mut reverse = seed.open_editor().unwrap();
    for update in &updates {
        forward.import(update).unwrap();
    }
    for update in updates.iter().rev() {
        reverse.import(update).unwrap();
    }
    assert_eq!(forward.blocks().unwrap(), reverse.blocks().unwrap());
    assert_eq!(
        forward.semantic_hash().unwrap(),
        reverse.semantic_hash().unwrap()
    );
    assert_eq!(forward.view().unwrap(), reverse.view().unwrap());
    forward
}

fn assert_unchanged(doc: &PlanDocument, revision: &PlanRevision, blocks: &[PlanBlock]) {
    assert_eq!(&doc.revision().unwrap(), revision);
    assert_eq!(doc.blocks().unwrap(), blocks);
}

mod admission;
mod behavior;
mod grammar;
mod revisions;
