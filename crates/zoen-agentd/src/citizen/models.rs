//! Cheap-tier chat through the Zoen-owned model gateway (`zoen-models`).
//!
//! Workers AI is used whenever `CLOUDFLARE_WORKERS_AI_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
//! are present. Each turn is one admitted attempt under [`CheapTierAuthority`], a bounded
//! per-owner daily cap. That is a staging guard, not the durable financial authority (U10).

use super::host::{parse_reply, Brain, BrainOut, ToolCall, ToolDef, ToolStep, Turn, TurnContext};
use super::profile::Definition;
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::Duration;
use zoen_models::{
    Admission, AdmissionError, AttemptContext, DispatchAuthority, DispatchRequest, EndpointPolicy,
    GatewayConfig, InputMessage, ModelGateway, ModelLimits, ModelRequest, Operation, OutputBlock,
    ToolDefinition,
};

pub const CHEAP_CHAT_PROFILE: &str = zoen_models::CHAT_PROFILE;
/// The cheap default tier on Workers AI (GLM-5.3-Flash, docs/research/unit-economics.md).
pub const CHEAP_MODEL: &str = "@cf/zai-org/glm-5.3-flash";

/// Admits each attempt id once and at most `daily_cap` attempts per owner per UTC day.
pub struct CheapTierAuthority {
    daily_cap: u32,
    seen: Mutex<BTreeSet<String>>,
    used: Mutex<BTreeMap<(String, i64), u32>>,
}

impl CheapTierAuthority {
    pub fn new(daily_cap: u32) -> Self {
        Self {
            daily_cap,
            seen: Mutex::default(),
            used: Mutex::default(),
        }
    }
}

#[async_trait]
impl DispatchAuthority for CheapTierAuthority {
    async fn admit(&self, r: &DispatchRequest) -> Result<Admission, AdmissionError> {
        // This staging quota is chat-only, never image billing authority.
        if r.profile != CHEAP_CHAT_PROFILE
            || r.descriptor.operation != Operation::ChatCompletion
            || r.descriptor.image.is_some()
        {
            return Err(AdmissionError::Denied);
        }
        if !self
            .seen
            .lock()
            .unwrap()
            .insert(r.context.attempt_id.clone())
        {
            return Ok(Admission::AlreadyKnown);
        }
        let day = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64 / 86_400)
            .unwrap_or(0);
        let mut used = self.used.lock().unwrap();
        let n = used.entry((r.context.owner.clone(), day)).or_insert(0);
        if *n >= self.daily_cap {
            return Err(AdmissionError::Denied);
        }
        *n += 1;
        Ok(Admission::Fresh)
    }
}

pub struct ModelBrain {
    gateway: ModelGateway,
    authority: CheapTierAuthority,
    model: String,
}

impl ModelBrain {
    /// Workers AI when the token and account are configured; `None` otherwise.
    pub fn workers_ai_from_env() -> Option<Self> {
        let key = std::env::var("CLOUDFLARE_WORKERS_AI_TOKEN")
            .ok()
            .filter(|k| !k.is_empty())?;
        let account = std::env::var("CLOUDFLARE_ACCOUNT_ID")
            .ok()
            .filter(|a| !a.is_empty())?;
        let model = std::env::var("ZOEN_CHEAP_MODEL").unwrap_or_else(|_| CHEAP_MODEL.into());
        let gateway = ModelGateway::new(GatewayConfig {
            base_url: format!("https://api.cloudflare.com/client/v4/accounts/{account}/ai/v1"),
            endpoint_policy: EndpointPolicy::Https,
            model: model.clone(),
            credential_ref: "cloudflare-workers-ai".into(),
            api_key: key,
            limits: ModelLimits {
                max_request_bytes: 256 * 1024,
                max_response_bytes: 1024 * 1024,
                max_output_tokens: 800,
                timeout: Duration::from_secs(60),
                connect_timeout: Duration::from_secs(10),
            },
        })
        .ok()?;
        Some(Self {
            gateway,
            authority: CheapTierAuthority::new(500),
            model,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl Brain for ModelBrain {
    async fn think(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<BrainOut, String> {
        self.think_with_tools(cx, history, inbound, &[], &[]).await
    }

    async fn think_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, String> {
        let (owner, agent, device, def) = (cx.owner, cx.agent, cx.device, cx.definition);
        let mut messages = vec![InputMessage::System {
            text: system_prompt(def),
        }];
        for t in history.iter().rev().take(16).rev() {
            if t.from == agent {
                messages.push(InputMessage::Assistant {
                    blocks: vec![OutputBlock::Text {
                        text: t.text.clone(),
                    }],
                });
            } else {
                messages.push(InputMessage::User {
                    text: t.text.clone(),
                });
            }
        }
        messages.push(InputMessage::User {
            text: inbound.text.clone(),
        });
        for st in steps {
            messages.push(InputMessage::Assistant {
                blocks: vec![OutputBlock::ToolCall {
                    call_id: st.call.id.clone(),
                    name: st.call.name.clone(),
                    arguments: st.call.arguments.clone(),
                }],
            });
            messages.push(InputMessage::ToolResult {
                call_id: st.call.id.clone(),
                name: st.call.name.clone(),
                text: st.result.clone(),
            });
        }
        let attempt = types::new_id("att").map_err(|error| error.to_string())?;
        let request = ModelRequest {
            context: AttemptContext {
                attempt_id: attempt.clone(),
                run_id: attempt,
                owner: owner.into(),
                agent: agent.into(),
                device: device.into(),
                space: inbound.space.clone(),
                authority_version: "cheap-tier-cap/1".into(),
                definition_version: def.version.to_string(),
                price_version: "workers-ai-free/1".into(),
            },
            operation: Operation::ChatCompletion,
            messages,
            tools: tools
                .iter()
                .map(|t| ToolDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect(),
            max_output_tokens: 800,
        };
        let result = self
            .gateway
            .complete(request, &self.authority)
            .await
            .map_err(|e| e.to_string())?;
        let blocks = result
            .output
            .map_err(|f| format!("model output failed: {f:?}"))?;
        let mut calls = Vec::new();
        let mut text = Vec::new();
        for b in blocks {
            match b {
                OutputBlock::Text { text: t } => text.push(t),
                OutputBlock::ToolCall {
                    call_id,
                    name,
                    arguments,
                } => calls.push(ToolCall {
                    requires_review: false,
                    id: call_id,
                    name,
                    arguments,
                }),
            }
        }
        if !calls.is_empty() {
            return Ok(BrainOut::CallTools(calls));
        }
        Ok(parse_reply(&text.join("\n")))
    }
}

pub fn system_prompt(def: &Definition) -> String {
    let mut s = def.persona.clone();
    if !def.rules.is_empty() {
        s.push_str("\n\nRules:\n");
        for r in &def.rules {
            s.push_str("- ");
            s.push_str(r);
            s.push('\n');
        }
    }
    s.push_str("\nYour state (JSON, you own it):\n");
    s.push_str(&def.state.to_string());
    s
}

#[cfg(test)]
mod tests;
