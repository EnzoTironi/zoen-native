//! Opt-in, closed failure metadata. Never Display, provider text, or authority.
use super::correlate;
use std::io::{self, Write};
use zoen_agentd::{HostError, TurnOutcome};

const EVENT: &str = "zoen.agent.failure.v1";
const MAX_LINE: usize = 512;

// Full static errors emitted by the Codex adapter, not patterns over its output.
// A matching String is only a diagnostic label, not proof of provider provenance.
const BRAIN_CODES: &[(&str, &str)] = &[
    ("invalid model input", "model_input_invalid"),
    ("codex-local input exceeds limit", "codex_input_limit"),
    (
        "Codex private working directory unavailable",
        "codex_work_unavailable",
    ),
    (
        "Codex executable unavailable",
        "codex_executable_unavailable",
    ),
    ("Codex output unavailable", "codex_output_unavailable"),
    (
        "unsupported Codex version; requalify adapter first",
        "codex_version_unsupported",
    ),
    (
        "Codex version process failed",
        "codex_version_process_failed",
    ),
    ("Codex version check failed", "codex_version_check_failed"),
    (
        "Codex MCP metadata unavailable",
        "codex_mcp_metadata_unavailable",
    ),
    ("invalid MCP configuration ID", "codex_mcp_id_invalid"),
    ("invalid MCP ID", "codex_mcp_id_encoding_invalid"),
    (
        "codex-local requires Codex-managed ChatGPT login; no API-key fallback",
        "codex_login_required",
    ),
    (
        "Codex model catalog unavailable",
        "codex_model_catalog_unavailable",
    ),
    (
        "requested/default Codex model unavailable in catalog",
        "codex_model_unavailable",
    ),
    (
        "Codex thread did not admit isolated ephemeral policy",
        "codex_thread_policy_refused",
    ),
    ("invalid Codex thread", "codex_thread_invalid"),
    ("invalid Codex turn", "codex_turn_invalid"),
    (
        "unexpected Codex request; no external execution allowed",
        "codex_request_unexpected",
    ),
    ("invalid Codex notification", "codex_notification_invalid"),
    (
        "Codex notification belongs to another thread",
        "codex_notification_thread_mismatch",
    ),
    ("invalid Codex item", "codex_item_invalid"),
    (
        "unexpected Codex action item; no execution granted",
        "codex_action_item_unexpected",
    ),
    ("invalid Codex reply", "codex_reply_invalid"),
    ("Codex reply exceeds limit", "codex_reply_limit"),
    ("Codex turn did not complete", "codex_turn_incomplete"),
    ("Codex inference failed", "codex_inference_failed"),
    (
        "Codex tool context exceeds limit",
        "codex_tool_context_limit",
    ),
    ("Codex local inference timed out", "codex_inference_timeout"),
    ("Codex output read failed", "codex_output_read_failed"),
    ("Codex process ended", "codex_process_ended"),
    ("Codex output exceeds limit", "codex_output_limit"),
    (
        "invalid Codex output encoding",
        "codex_output_encoding_invalid",
    ),
    ("Codex config unavailable", "codex_config_unavailable"),
    (
        "Codex ambient notification command was not disabled",
        "codex_notify_not_disabled",
    ),
    (
        "Codex ambient capabilities were not disabled",
        "codex_capabilities_not_disabled",
    ),
    (
        "Codex app-server unavailable",
        "codex_app_server_unavailable",
    ),
    ("Codex stdin unavailable", "codex_stdin_unavailable"),
    ("Codex stdout unavailable", "codex_stdout_unavailable"),
    ("invalid Codex request", "codex_request_invalid"),
    ("Codex request exceeds limit", "codex_request_limit"),
    ("Codex request write failed", "codex_request_write_failed"),
    ("invalid Codex protocol JSON", "codex_protocol_json_invalid"),
    (
        "unexpected Codex request/response",
        "codex_rpc_response_unexpected",
    ),
    ("Codex RPC refused", "codex_rpc_refused"),
    (
        "unexpected Codex item during admission",
        "codex_admission_item_unexpected",
    ),
    ("invalid Codex proposal", "codex_proposal_invalid"),
    (
        "Codex proposed an unoffered/invalid tool",
        "codex_tool_unoffered_or_invalid",
    ),
    (
        "invalid Codex tool arguments",
        "codex_tool_arguments_invalid",
    ),
    (
        "invalid Codex proposal outcome",
        "codex_proposal_outcome_invalid",
    ),
];

fn enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

fn classification(error: &HostError) -> (&'static str, &'static str) {
    match error {
        HostError::Unknown => ("unknown_agent", "unknown"),
        HostError::NotMember => ("not_member", "unknown"),
        HostError::ToolContinuation(_) => ("tool_continuation", "unknown"),
        HostError::ToolAdmission(_) => ("tool_admission", "unknown"),
        HostError::Brain(message) => (
            "brain",
            BRAIN_CODES
                .iter()
                .find_map(|(exact, code)| (message == exact).then_some(*code))
                .unwrap_or("unknown"),
        ),
    }
}

fn write_result(
    on: bool,
    space: &str,
    source: &str,
    result: &Result<TurnOutcome, HostError>,
    writer: &mut impl Write,
) {
    let Err(error) = result else { return };
    if !on {
        return;
    }
    let (variant, code) = classification(error);
    // Fixed keys and BTreeMap order are stable across serde_json map features.
    // This also needs no production dependency on the serde derive macros.
    let value = std::collections::BTreeMap::from([
        ("event", EVENT.into()),
        ("correlation", correlate(space, source)),
        ("phase", "turn_result".into()),
        ("variant", variant.into()),
        ("code", code.into()),
    ]);
    // Serialize only closed strings + the existing locator hash. Future growth
    // drops this diagnostic; it must not affect the result, admission or retries.
    if let Ok(mut line) = serde_json::to_vec(&value) {
        line.push(b'\n');
        if line.len() <= MAX_LINE {
            let _ = writer.write_all(&line);
        }
    }
}

pub(super) fn turn_result(space: &str, source: &str, result: &Result<TurnOutcome, HostError>) {
    let on = enabled(std::env::var("ZOEN_AGENT_OBSERVE_FAILURES").ok().as_deref());
    if on {
        write_result(true, space, source, result, &mut io::stdout().lock());
    }
}

#[cfg(test)]
mod tests;
