//! Shared authenticated Plan fixtures; behavior suites retain the original cases.

use super::super::{
    read_nodes, BlockKind, CanonicalPlanSeed, ExpectedLine, LegacyVersion, LinePatch, PlanCommand,
    PlanDocument,
};
use super::json::{MAX_JSON_DEPTH, MAX_JSON_NODES};
use super::*;
use loro::{ContainerTrait, ExportMode, JsonChange};
use loro::{IdSpan, JsonMapOp, JsonOp, JsonOpContent, JsonSchema, JsonTextOp, LoroValue, ID};
use types::{PlanDoc, PlanLine, PlanSection};

pub(super) fn seed() -> CanonicalPlanSeed {
    CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "creation".into(),
        document: PlanDoc {
            title: "Trip".into(),
            summary: String::new(),
            budget_cents: None,
            sections: vec![PlanSection {
                title: "Stay".into(),
                lines: vec![PlanLine {
                    id: "original".into(),
                    text: "abcdefghi".into(),
                    cost_cents: 100,
                    done: false,
                }],
            }],
        },
    }])
    .unwrap()
}

pub(super) fn line_id(doc: &PlanDocument) -> String {
    doc.view().unwrap().sections[0].lines[0].block_id.clone()
}

pub(super) fn change_cost(doc: &mut PlanDocument, cents: i64) {
    doc.apply(PlanCommand::ChangeLine {
        block: line_id(doc),
        expected: ExpectedLine::AnyCurrent,
        patch: LinePatch {
            cost_cents: Some(cents),
            ..Default::default()
        },
    })
    .unwrap();
}

pub(super) fn raw_packet(schema: &JsonSchema) -> Vec<u8> {
    serde_json::to_vec(&EncodedEnvelope {
        format: FORMAT,
        version: VERSION,
        operations: schema,
    })
    .unwrap()
}

pub(super) fn split(doc: &PlanDocument, update: &PlanUpdate) -> PlanUpdate {
    let mut changes = Vec::new();
    for change in &update.0.changes {
        for counter in
            change.id.counter..change.id.counter + i32::try_from(change.op_len()).unwrap()
        {
            changes.extend(doc.page.doc.export_json_in_id_span(IdSpan::new(
                change.id.peer,
                counter,
                counter + 1,
            )));
        }
    }
    PlanUpdate(JsonSchema {
        changes,
        ..update.0.clone()
    })
}

pub(super) fn unchanged(
    doc: &PlanDocument,
    vv: &super::super::PlanVersion,
    view: &super::super::PlanView,
) {
    assert_eq!(&doc.version(), vv);
    assert_eq!(&doc.view().unwrap(), view);
}

use serde_json::Value;

mod causality;
mod envelope_limits;
mod grammar;
mod normalization;
mod overlaps;
