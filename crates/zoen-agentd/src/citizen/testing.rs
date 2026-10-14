//! Journey-only seams. Without the `scripted` feature (every production build) the scripted
//! brain doesn't exist and fault points are no-ops.

use super::host::Brain;

/// `ZOEN_AGENT_SCRIPT` (a JSON map of substring → reply) as a brain, in journey builds.
pub fn scripted_brain_from_env() -> Option<Result<Box<dyn Brain>, String>> {
    #[cfg(feature = "scripted")]
    {
        let path = std::env::var("ZOEN_AGENT_SCRIPT").ok()?;
        Some(
            std::fs::read_to_string(&path)
                .map_err(|e| format!("script: {e}"))
                .and_then(|t| serde_json::from_str(&t).map_err(|e| format!("script: {e}")))
                .map(|replies| Box::new(scripted::ScriptedBrain { replies }) as Box<dyn Brain>),
        )
    }
    #[cfg(not(feature = "scripted"))]
    None
}

/// Exits the process at `point` when `ZOEN_AGENT_FAULT` names it (journey builds only):
/// how journeys prove a restart mid-turn neither loses nor repeats a reply.
pub fn fault(point: &str) {
    #[cfg(feature = "scripted")]
    if std::env::var("ZOEN_AGENT_FAULT").as_deref() == Ok(point) {
        let marker = std::env::var("ZOEN_AGENT_FAULT_MARKER").ok();
        // Fire once: the restarted runtime runs clean.
        if let Some(m) = &marker {
            if std::path::Path::new(m).exists() {
                return;
            }
            let _ = std::fs::write(m, point);
        }
        std::process::exit(86);
    }
    #[cfg(not(feature = "scripted"))]
    let _ = point;
}

#[cfg(feature = "scripted")]
pub mod scripted {
    use super::super::host::{parse_reply, Brain, BrainOut, Turn, TurnContext};
    use async_trait::async_trait;
    use std::collections::BTreeMap;

    /// Fixed replies keyed by a case-insensitive substring of the inbound text. Parsed like
    /// model output; no match is silence.
    #[derive(Default)]
    pub struct ScriptedBrain {
        pub replies: BTreeMap<String, String>,
    }

    #[async_trait]
    impl Brain for ScriptedBrain {
        async fn think(
            &self,
            _cx: TurnContext<'_>,
            _history: &[Turn],
            inbound: &Turn,
        ) -> Result<BrainOut, String> {
            let t = inbound.text.to_lowercase();
            Ok(self
                .replies
                .iter()
                .find(|(k, _)| t.contains(&k.to_lowercase()))
                .map(|(_, v)| parse_reply(v))
                .unwrap_or(BrainOut::Silence))
        }
    }
}
