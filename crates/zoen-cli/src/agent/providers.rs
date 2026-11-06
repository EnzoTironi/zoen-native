//! Existing provider construction and bounded HTTP transport for the local host.

use super::*;

/// Clef (clef-flash) on Workers AI. Without credentials the runtime refuses to start
/// unless `ZOEN_AGENT_GATE=offline` explicitly accepts the hard rules + offline floor.
pub(super) fn gate() -> Result<Gate, String> {
    if std::env::var("ZOEN_AGENT_GATE").as_deref() == Ok("offline") {
        return Ok(Gate::default());
    }
    match (env("CLOUDFLARE_WORKERS_AI_TOKEN"), env("CLOUDFLARE_ACCOUNT_ID")) {
        (Some(token), Some(account)) => Ok(Gate {
            remote: Some(Box::new(Clef {
                transport: Arc::new(ReqwestTransport::new()?),
                account,
                token,
                model: ClefModel::Flash,
            })),
            ..Gate::default()
        }),
        _ => Err(
            "no gate: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID, or ZOEN_AGENT_GATE=offline"
                .into(),
        ),
    }
}

pub(super) fn brain() -> Result<Box<dyn Brain>, String> {
    if let Some(scripted) = scripted_brain_from_env() {
        return scripted;
    }
    match env("ZOEN_BRAIN").as_deref() {
        Some("codex-local") => CodexBrain::from_env().map(|b| Box::new(b) as Box<dyn Brain>),
        None | Some("workers-ai") => ModelBrain::workers_ai_from_env()
            .map(|b| Box::new(b) as Box<dyn Brain>)
            .ok_or_else(|| {
                "no model: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID".into()
            }),
        Some(_) => Err("unsupported ZOEN_BRAIN; use workers-ai or codex-local".into()),
    }
}
