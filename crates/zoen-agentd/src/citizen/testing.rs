//! Journey-only seams. Without the `scripted` feature (every production build) the scripted
//! brain doesn't exist and fault points are no-ops.

use super::host::Brain;

/// Test-only admission hooks around an actual Brain. Hooks may reject, never
/// replace its output. Production builds do not enable the scripted feature.
#[cfg(feature = "scripted")]
pub type BrainAdmission = dyn Fn() -> Result<(), String> + Send + Sync;
#[cfg(feature = "scripted")]
pub type BrainOutputAdmission =
    dyn Fn(&[super::host::ToolStep], &super::host::BrainOut) -> Result<(), String> + Send + Sync;
#[cfg(feature = "scripted")]
pub struct GuardedBrain {
    pub inner: Box<dyn Brain>,
    pub before: std::sync::Arc<BrainAdmission>,
    pub after: std::sync::Arc<BrainOutputAdmission>,
}
#[cfg(feature = "scripted")]
#[async_trait::async_trait]
impl Brain for GuardedBrain {
    async fn think(
        &self,
        cx: super::host::TurnContext<'_>,
        history: &[super::host::Turn],
        inbound: &super::host::Turn,
    ) -> Result<super::host::BrainOut, String> {
        self.think_with_tools(cx, history, inbound, &[], &[]).await
    }
    async fn think_with_tools(
        &self,
        cx: super::host::TurnContext<'_>,
        history: &[super::host::Turn],
        inbound: &super::host::Turn,
        tools: &[super::host::ToolDef],
        steps: &[super::host::ToolStep],
    ) -> Result<super::host::BrainOut, String> {
        (self.before)()?;
        let out = self
            .inner
            .think_with_tools(cx, history, inbound, tools, steps)
            .await?;
        (self.after)(steps, &out)?;
        Ok(out)
    }
}

/// Synchronous controlled output for offline integration tests. No provider,
/// network, environment or credentials; absent from every production build.
#[cfg(feature = "scripted")]
pub type ControlledToolEvaluation =
    dyn Fn(&[super::host::ToolStep]) -> Result<super::host::BrainOut, String> + Send + Sync;

#[cfg(feature = "scripted")]
pub struct ControlledToolBrain {
    pub evaluate: Box<ControlledToolEvaluation>,
}
#[cfg(feature = "scripted")]
#[async_trait::async_trait]
impl Brain for ControlledToolBrain {
    async fn think(
        &self,
        _: super::host::TurnContext<'_>,
        _: &[super::host::Turn],
        _: &super::host::Turn,
    ) -> Result<super::host::BrainOut, String> {
        (self.evaluate)(&[])
    }
    async fn think_with_tools(
        &self,
        _: super::host::TurnContext<'_>,
        _: &[super::host::Turn],
        _: &super::host::Turn,
        _: &[super::host::ToolDef],
        steps: &[super::host::ToolStep],
    ) -> Result<super::host::BrainOut, String> {
        (self.evaluate)(steps)
    }
}

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
    use super::super::host::{
        parse_reply, Brain, BrainOut, ToolCall, ToolDef, ToolStep, Turn, TurnContext,
    };
    use async_trait::async_trait;
    use std::collections::BTreeMap;

    /// Fixed replies keyed by a case-insensitive substring of the inbound text. Parsed like
    /// model output; no match is silence.
    ///
    /// A reply may carry a fenced ```tools block (a JSON array of `{"name", "arguments"}`):
    /// a deterministic stand-in for a model that calls tools. The first think makes those
    /// calls; once their results are in, it says the rest of the reply, or the refusal when
    /// a call failed.
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
            self.think_with_tools(_cx, _history, inbound, &[], &[])
                .await
        }

        async fn think_with_tools(
            &self,
            _cx: TurnContext<'_>,
            _history: &[Turn],
            inbound: &Turn,
            tools: &[ToolDef],
            steps: &[ToolStep],
        ) -> Result<BrainOut, String> {
            let t = inbound.text.to_lowercase();
            let Some((_, v)) = self
                .replies
                .iter()
                .find(|(k, _)| t.contains(&k.to_lowercase()))
            else {
                return Ok(BrainOut::Silence);
            };
            let Some((before, json, after)) = fence(v) else {
                return Ok(parse_reply(v));
            };
            if let Some(failed) = steps.iter().find(|s| s.is_error) {
                // The words of an MCP tool result, not its structured payload.
                let v: serde_json::Value = serde_json::from_str(&failed.result).unwrap_or_default();
                let why = v["content"][0]["text"].as_str().unwrap_or(&failed.result);
                return Ok(BrainOut::Say(format!("Não deu: {why}")));
            }
            if steps.is_empty() && !tools.is_empty() {
                let calls: Vec<serde_json::Value> =
                    serde_json::from_str(json).map_err(|e| format!("script tools: {e}"))?;
                return Ok(BrainOut::CallTools(
                    calls
                        .into_iter()
                        .enumerate()
                        .map(|(i, c)| ToolCall {
                            requires_review: false,
                            id: format!("call_{i}"),
                            name: c["name"].as_str().unwrap_or_default().to_string(),
                            arguments: c["arguments"].clone(),
                        })
                        .collect(),
                ));
            }
            Ok(parse_reply(&format!("{}{}", before, after)))
        }
    }

    fn fence(text: &str) -> Option<(&str, &str, &str)> {
        let start = text.find("```tools")?;
        let rest = &text[start + 8..];
        let end = rest.find("```")?;
        Some((&text[..start], rest[..end].trim(), &rest[end + 3..]))
    }
}
