//! Opt-in, closed metadata from the actual model/host boundaries. Never payloads.
use super::correlate;
use crate::agent::SpaceTools;
use serde_json::{json, Value};
use std::io::{self, Write};
use zoen_agentd::{ToolCall, ToolDef, ToolObservation, ToolStep};

const EVENT: &str = "zoen.agent.tools.v1";
const KINDS: &[&str] = &[
    "create_site",
    "read_site",
    "replace_site_source",
    "create_deckcraft",
    "create_gridcraft",
    "create_wordcraft",
    "create_photocraft",
    "read_photocraft",
    "photocraft_transform",
    "read_deckcraft",
    "read_gridcraft",
    "gridcraft_cell_set",
    "read_wordcraft",
    "wordcraft_text_replace",
    "deckcraft_text_set",
    "deckcraft_slide_notes",
    "deckcraft_shape_insert",
    "deckcraft_slide_new",
    "read_page",
    "replace_page_block",
];

fn enabled(value: Option<&str>) -> bool {
    value == Some("1")
}
fn opt_in() -> bool {
    enabled(std::env::var("ZOEN_AGENT_OBSERVE_TOOLS").ok().as_deref())
}

fn public_kind(host: &SpaceTools<'_>, name: &str) -> Option<Value> {
    // A model/user-controlled name is never evidence of a native artifact.
    // Discovery records native item membership separately from generic App tools.
    let fresh = host.fresh();
    let current = if host.names.iter().any(|(def, _, _)| def.name == name) {
        host
    } else {
        &fresh
    };
    let (_, item, kind) = current.names.iter().find(|(def, _, _)| def.name == name)?;
    if client_core::agent_app_creation::is_model_name(kind) && item == &host.space {
        return (client_core::agent_app_creation::model_name_for(kind, &host.space).as_deref()
            == Some(name))
        .then(|| json!({"kind":kind}));
    }
    if kind == "create_site" && item == &host.space {
        return (client_core::agent_sites::model_name(kind, item).as_deref() == Some(name))
            .then(|| json!({"kind":kind}));
    }
    let (expected, separator) = if current
        .e
        .agent_site_sources(&current.space, current.source, current.serving)
        .iter()
        .any(|(id, _, _)| id == item)
        && current.names.iter().any(|(definition, locator, tool)| {
            definition.name == name
                && locator == item
                && client_core::agent_sites::model_name(tool, item).as_deref() == Some(name)
        }) {
        (client_core::agent_sites::model_name(kind, item)?, "_")
    } else if current.deckcraft_items.contains(item) && crate::deckcraft::recognizes(kind) {
        (crate::deckcraft::model_name(kind, item)?, "__")
    } else if current.page_items.contains(item) && client_core::agent_page_tools::recognizes(kind) {
        (client_core::agent_page_tools::model_name(kind, item)?, "_")
    } else {
        return None;
    };
    if expected != name || !KINDS.contains(&kind.as_str()) {
        return None;
    }
    let alias = expected.strip_prefix(&format!("{kind}{separator}"))?;
    Some(json!({"kind":kind,"artifactAlias":alias}))
}

fn input_record(
    on: bool,
    space: &str,
    source: &str,
    tools: &[ToolDef],
    steps: &[ToolStep],
    round: usize,
    host: &SpaceTools<'_>,
) -> Option<Value> {
    if !on
        || tools.len() > 128
        || steps.len() > zoen_agentd::MAX_TOOL_STEPS
        || round > zoen_agentd::citizen::MAX_TOOL_ROUNDS
    {
        return None;
    }
    let recognized: Vec<_> = tools
        .iter()
        .filter_map(|t| public_kind(host, &t.name))
        .collect();
    Some(
        json!({"event":EVENT,"correlation":correlate(space,source),"phase":"model_input",
        "roundIndex":round,"offeredCount":tools.len(),"unclassifiedCount":tools.len()-recognized.len(),
        "recognizedTools":recognized,"completedSteps":steps.len(),"failedSteps":steps.iter().filter(|s|s.is_error).count()}),
    )
}
fn result_record(
    on: bool,
    space: &str,
    source: &str,
    call: &ToolCall,
    outcome: ToolObservation,
    host: &SpaceTools<'_>,
) -> Option<Value> {
    if !on {
        return None;
    }
    let class = match outcome {
        ToolObservation::Held => "held",
        ToolObservation::Blocked => "blocked",
        ToolObservation::UnknownTool => "unknown_tool",
        ToolObservation::PreparationRefused => "preparation_refused",
        ToolObservation::AdmissionError => "admission_error",
        ToolObservation::HostOk => "host_ok",
        ToolObservation::HostError => "host_error",
    };
    Some(
        json!({"event":EVENT,"correlation":correlate(space,source),"phase":"tool_outcome",
        "resultClass":class,"requiresReview":call.requires_review,"tool":if outcome == ToolObservation::UnknownTool { json!({"kind":"other"}) } else { public_kind(host, &call.name).unwrap_or(json!({"kind":"other"})) }}),
    )
}
fn emit_to(record: &Value, writer: &mut impl Write) {
    if serde_json::to_writer(&mut *writer, record).is_ok() {
        let _ = writer.write_all(b"\n");
    }
}
pub(in crate::agent) fn model_input(
    host: &SpaceTools<'_>,
    tools: &[ToolDef],
    steps: &[ToolStep],
    round: usize,
) {
    if let Some(record) = input_record(
        opt_in(),
        &host.space,
        host.source,
        tools,
        steps,
        round,
        host,
    ) {
        emit_to(&record, &mut io::stdout().lock());
    }
}
pub(in crate::agent) fn tool_outcome(
    host: &SpaceTools<'_>,
    call: &ToolCall,
    outcome: ToolObservation,
) {
    if let Some(record) = result_record(opt_in(), &host.space, host.source, call, outcome, host) {
        emit_to(&record, &mut io::stdout().lock());
    }
}

#[cfg(test)]
mod tests;
