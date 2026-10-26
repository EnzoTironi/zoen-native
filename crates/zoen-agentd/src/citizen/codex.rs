//! Local, opt-in Codex-managed ChatGPT inference. Not a hosted/commercial auth path.
//! Codex only produces JSON proposals. Runtime remains the sole Gate/ToolHost owner.
//! No credentials are read, copied or returned by this adapter.
use super::host::{
    parse_reply, Brain, BrainError, BrainOut, ToolCall, ToolDef, ToolStep, Turn, TurnContext,
};
use super::models::system_prompt;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

const VERSION: &str = "codex-cli 0.164.0";
const MAX_INPUT: usize = 256 * 1024;
const MAX_FRAME: usize = 1024 * 1024;
const MAX_WIRE: usize = 4 * 1024 * 1024;
const DISABLED: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "shell_snapshot",
    "apps",
    "plugins",
    "memories",
    "hooks",
    "multi_agent",
    "view_image",
    "image_generation",
    "browser_use",
    "computer_use",
    "code_mode",
    "code_mode_host",
    "in_app_browser",
    "workspace_dependencies",
    "request_permissions_tool",
    "skill_search",
    "in_app_local_automation",
    "in_app_voice",
    "in_app_chat",
    "in_app_dictation",
    "tool_suggest",
    "sleep_tool",
    "realtime_conversation",
];

/// Local testing/development only. The installed Codex owns its managed login;
/// the operator explicitly selects this provider instead of Workers AI. No fallback.
/// Commercial/hosted deployment requires the separate Sign in with ChatGPT flow.
#[derive(Clone)]
pub struct CodexBrain {
    binary: PathBuf,
    model: Option<String>,
    timeout: Duration,
}
impl CodexBrain {
    pub fn from_env() -> Result<Self, String> {
        let binary = std::env::var_os("ZOEN_CODEX_BIN")
            .map(PathBuf::from)
            .ok_or("codex-local requires an absolute ZOEN_CODEX_BIN")?;
        if !binary.is_absolute() || !binary.is_file() {
            return Err("codex-local requires an existing absolute ZOEN_CODEX_BIN".into());
        }
        let model = std::env::var("ZOEN_CODEX_MODEL").ok();
        if model.as_ref().is_some_and(|s| !token(s, 128)) {
            return Err("invalid ZOEN_CODEX_MODEL".into());
        }
        Ok(Self {
            binary,
            model,
            timeout: Duration::from_secs(120),
        })
    }

    async fn complete(
        &self,
        instructions: String,
        input: Value,
        tools: &[ToolDef],
    ) -> Result<BrainOut, String> {
        let text = serde_json::to_string(&input).map_err(|_| "invalid model input")?;
        if text.len() + instructions.len() > MAX_INPUT {
            return Err("codex-local input exceeds limit".into());
        }
        let work = Work::new()?;
        let mut version = command(&self.binary);
        version
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = version
            .spawn()
            .map_err(|_| "Codex executable unavailable")?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or("Codex output unavailable")?);
        let mut bytes = 0;
        let found = line(&mut stdout, &mut bytes).await?;
        if found.trim() != VERSION {
            return Err("unsupported Codex version; requalify adapter first".into());
        }
        if !child
            .wait()
            .await
            .map_err(|_| "Codex version process failed")?
            .success()
        {
            return Err("Codex version check failed".into());
        }
        // An empty table is a config merge, not a deny-all. Discover IDs via
        // metadata, then override each enabled field and verify the final config.
        let mut discovery = Session::start(&self.binary, &work.0, None).await?;
        let config = discovery
            .rpc("config/read", json!({"cwd":work.0,"includeLayers":false}))
            .await?;
        let servers = config
            .pointer("/config/mcp_servers")
            .and_then(Value::as_object)
            .ok_or("Codex MCP metadata unavailable")?;
        let mut entries = Vec::new();
        for key in servers.keys() {
            if key.len() > 256 || key.chars().any(char::is_control) {
                return Err("invalid MCP configuration ID".into());
            }
            entries.push(format!(
                "{}={{enabled=false}}",
                serde_json::to_string(key).map_err(|_| "invalid MCP ID")?
            ));
        }
        let deny = format!("mcp_servers={{{}}}", entries.join(","));
        discovery.close().await;
        let mut session = Session::start(&self.binary, &work.0, Some(&deny)).await?;
        let config = session
            .rpc("config/read", json!({"cwd":work.0,"includeLayers":false}))
            .await?;
        validate_config(&config)?;
        let account = session
            .rpc("account/read", json!({"refreshToken":false}))
            .await?;
        if account.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt") {
            return Err(
                "codex-local requires Codex-managed ChatGPT login; no API-key fallback".into(),
            );
        }
        let catalog = session
            .rpc("model/list", json!({"limit":100,"includeHidden":false}))
            .await?;
        let models = catalog
            .get("data")
            .and_then(Value::as_array)
            .ok_or("Codex model catalog unavailable")?;
        let model = if let Some(requested) = &self.model {
            models
                .iter()
                .find(|m| m.get("id").and_then(Value::as_str) == Some(requested))
        } else {
            models
                .iter()
                .find(|m| m.get("isDefault").and_then(Value::as_bool) == Some(true))
        }
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .filter(|m| token(m, 128))
        .ok_or("requested/default Codex model unavailable in catalog")?
        .to_string();
        let thread = session
            .rpc(
                "thread/start",
                json!({
                    "modelProvider":"openai","model":model,"allowProviderModelFallback":false,
                    "cwd":work.0,"sandbox":"read-only","approvalPolicy":"never",
                    "environments":[],"runtimeWorkspaceRoots":[],"selectedCapabilityRoots":[],
                    "ephemeral":true,"dynamicTools":[],"baseInstructions":instructions,
                    "developerInstructions":PROPOSAL_GUIDANCE
                }),
            )
            .await?;
        if thread.pointer("/thread/ephemeral").and_then(Value::as_bool) != Some(true)
            || thread.pointer("/sandbox/type").and_then(Value::as_str) != Some("readOnly")
            || thread
                .pointer("/sandbox/networkAccess")
                .and_then(Value::as_bool)
                != Some(false)
            || thread.get("modelProvider").and_then(Value::as_str) != Some("openai")
            || thread.get("model").and_then(Value::as_str) != Some(&model)
            || thread
                .get("instructionSources")
                .and_then(Value::as_array)
                .is_none_or(|a| !a.is_empty())
        {
            return Err("Codex thread did not admit isolated ephemeral policy".into());
        }
        let thread_id = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .filter(|s| token(s, 128))
            .ok_or("invalid Codex thread")?
            .to_string();
        let turn = session
            .rpc(
                "turn/start",
                json!({"threadId":thread_id,
                    "environments":[],"approvalPolicy":"never","effort":"low",
                    "input":[{"type":"text","text":text}],"outputSchema":output_schema(tools)
                }),
            )
            .await?;
        let turn_id = turn
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .filter(|s| token(s, 128))
            .ok_or("invalid Codex turn")?
            .to_string();
        let mut output = String::new();
        loop {
            let notification = session.next().await?;
            if notification.get("id").is_some() {
                return Err("unexpected Codex request; no external execution allowed".into());
            }
            let Some(method) = notification.get("method").and_then(Value::as_str) else {
                return Err("invalid Codex notification".into());
            };
            if let Some(actual) = notification
                .pointer("/params/threadId")
                .and_then(Value::as_str)
            {
                if actual != thread_id {
                    return Err("Codex notification belongs to another thread".into());
                }
            }
            match method {
                "item/started" | "item/completed" => {
                    let kind = notification
                        .pointer("/params/item/type")
                        .and_then(Value::as_str)
                        .ok_or("invalid Codex item")?;
                    if !matches!(kind, "agentMessage" | "userMessage" | "reasoning") {
                        return Err("unexpected Codex action item; no execution granted".into());
                    }
                    if method == "item/completed" && kind == "agentMessage" {
                        let text = notification
                            .pointer("/params/item/text")
                            .and_then(Value::as_str)
                            .ok_or("invalid Codex reply")?;
                        if output.len() + text.len() > 64 * 1024 {
                            return Err("Codex reply exceeds limit".into());
                        }
                        output.push_str(text);
                    }
                }
                "turn/completed" => {
                    if notification
                        .pointer("/params/turn/id")
                        .and_then(Value::as_str)
                        != Some(&turn_id)
                        || notification
                            .pointer("/params/turn/status")
                            .and_then(Value::as_str)
                            != Some("completed")
                    {
                        return Err("Codex turn did not complete".into());
                    }
                    break;
                }
                "error" => return Err("Codex inference failed".into()),
                _ => {}
            }
        }
        let result = parse_output(&output, tools);
        session.close().await;
        result
    }
}

#[async_trait]
impl Brain for CodexBrain {
    async fn think(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
    ) -> Result<BrainOut, BrainError> {
        self.think_with_tools(cx, history, inbound, &[], &[]).await
    }
    async fn think_with_tools(
        &self,
        cx: TurnContext<'_>,
        history: &[Turn],
        inbound: &Turn,
        tools: &[ToolDef],
        steps: &[ToolStep],
    ) -> Result<BrainOut, BrainError> {
        if tools.len() > 128
            || steps.len() > super::host::MAX_TOOL_ROUNDS * super::host::MAX_CALLS_PER_ROUND
            || tools.iter().any(|t| !token(&t.name, 128))
        {
            return Err(BrainError::NotDispatched(
                "Codex tool context exceeds limit".into(),
            ));
        }
        let history: Vec<Value> = history
            .iter()
            .rev()
            .take(16)
            .rev()
            .map(|t| json!({"from":t.from,"text":t.text}))
            .collect();
        let input = json!({"agent":cx.agent,"owner":cx.owner,"space":inbound.space,"history":history,
            "inbound":{"from":inbound.from,"text":inbound.text},"offered_tools":tools,"completed_tool_steps":steps});
        tokio::time::timeout(
            self.timeout,
            self.complete(system_prompt(cx.definition), input, tools),
        )
        .await
        .map_err(|_| BrainError::OutcomeUnknown("Codex local inference timed out".into()))?
        .map_err(BrainError::OutcomeUnknown)
    }
}

fn token(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}
fn command(binary: &PathBuf) -> Command {
    let mut c = Command::new(binary);
    c.env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .kill_on_drop(true);
    for key in ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "CODEX_HOME"] {
        if let Some(value) = std::env::var_os(key) {
            c.env(key, value);
        }
    }
    c
}
struct Work(PathBuf);
impl Work {
    fn new() -> Result<Self, String> {
        let directory = types::new_id("zoen-codex-brain").map_err(|error| {
            format!("Codex private working directory identifier unavailable: {error}")
        })?;
        let path = std::env::temp_dir().join(directory);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|_| "Codex private working directory unavailable")?;
        Ok(Self(path))
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Session {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    bytes: usize,
    next_id: u32,
    queued: std::collections::VecDeque<Value>,
}
mod session;
async fn line(reader: &mut BufReader<ChildStdout>, total: &mut usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader
            .fill_buf()
            .await
            .map_err(|_| "Codex output read failed")?;
        if chunk.is_empty() {
            return Err("Codex process ended".into());
        }
        let n = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |p| p + 1);
        if bytes.len() + n > MAX_FRAME || *total + n > MAX_WIRE {
            return Err("Codex output exceeds limit".into());
        }
        bytes.extend_from_slice(&chunk[..n]);
        *total += n;
        reader.consume(n);
        if bytes.last() == Some(&b'\n') {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|_| "invalid Codex output encoding".into())
}
fn validate_config(value: &Value) -> Result<(), String> {
    let c = value.get("config").ok_or("Codex config unavailable")?;
    // notify commands run outside the model's tool registry too. Never inherit
    // an operator's shell callback into this isolated proposal-only invocation.
    if c.get("notify")
        .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err("Codex ambient notification command was not disabled".into());
    }
    if DISABLED.iter().any(|flag| {
        c.get("features")
            .and_then(|f| f.get(*flag))
            .and_then(Value::as_bool)
            != Some(false)
    }) || c.get("web_search").and_then(Value::as_str) != Some("disabled")
        || c.get("project_doc_max_bytes").and_then(Value::as_u64) != Some(0)
        || c.get("mcp_servers")
            .and_then(Value::as_object)
            .is_none_or(|servers| {
                servers
                    .values()
                    .any(|s| s.get("enabled").and_then(Value::as_bool) != Some(false))
            })
    {
        return Err("Codex ambient capabilities were not disabled".into());
    }
    Ok(())
}
mod output_schema;
use output_schema::{output_schema, PROPOSAL_GUIDANCE};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    outcome: Outcome,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Outcome {
    Say { text: String },
    Quiet {},
    Call { calls: Vec<Call> },
    Review { calls: Vec<Call> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    id: String,
    name: String,
    arguments_json: String,
}
fn parse_output(text: &str, tools: &[ToolDef]) -> Result<BrainOut, String> {
    let reply: Reply = serde_json::from_str(text).map_err(|_| "invalid Codex proposal")?;
    let (requires_review, proposed) = match reply.outcome {
        Outcome::Say { text } => return Ok(parse_reply(&text)),
        Outcome::Quiet {} => return Ok(BrainOut::Silence),
        Outcome::Call { calls } => (false, calls),
        Outcome::Review { calls } => (true, calls),
    };
    if proposed.is_empty() || proposed.len() > super::host::MAX_CALLS_PER_ROUND {
        return Err("invalid Codex proposal outcome".into());
    }
    let mut calls = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for call in proposed {
        if !token(&call.id, 128)
            || !ids.insert(call.id.clone())
            || !tools.iter().any(|t| t.name == call.name)
        {
            return Err("Codex proposed an unoffered/invalid tool".into());
        }
        let arguments: Value = serde_json::from_str(&call.arguments_json)
            .map_err(|_| "invalid Codex tool arguments")?;
        if !arguments.is_object() {
            return Err("invalid Codex tool arguments".into());
        }
        calls.push(ToolCall {
            requires_review,
            id: call.id,
            name: call.name,
            arguments,
        });
    }
    Ok(BrainOut::CallTools(calls))
}

#[cfg(all(test, unix))]
#[path = "codex/tests.rs"]
mod tests;
