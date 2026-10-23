//! Trusted subprocess consumer of the managed image Runtime. The ordinary
//! citizen keeps its own MLS device; only public locators/content cross this port.
use super::*;
use crate::image_protocol::{Command, Progress, MAX_INPUT, MAX_OUTPUT};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(test)]
mod tests;

pub(super) const TOOL: &str = "generate_image";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    prompt: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    schema: String,
    space: String,
    source: String,
    progress: Progress,
}
fn paths() -> Result<(PathBuf, PathBuf), String> {
    let binary = PathBuf::from(env("ZOEN_IMAGE_HOST_BIN").ok_or("image host unavailable")?);
    let config =
        PathBuf::from(env("ZOEN_IMAGE_HOST_CONFIG").ok_or("image configuration unavailable")?);
    if !binary.is_absolute() || !config.is_absolute() {
        return Err("image host paths must be absolute".into());
    }
    Ok((binary, config))
}
pub(super) fn configured() -> bool {
    paths().is_ok()
}
pub(super) fn definition() -> ToolDef {
    ToolDef {
        name: TOOL.into(),
        description: "Propose an image in this conversation. The owner reviews the exact specification and price before generation.".into(),
        parameters: json!({"type":"object","properties":{"prompt":{"type":"string","minLength":1,"maxLength":8192}},"required":["prompt"],"additionalProperties":false}),
    }
}
async fn execute(command: Command) -> Result<Progress, String> {
    command.validate()?;
    let (binary, config) = paths()?;
    let bytes = serde_json::to_vec(&command).map_err(|_| "invalid image command")?;
    if bytes.len() > MAX_INPUT {
        return Err("image command too large".into());
    }
    let mut process = tokio::process::Command::new(binary);
    process
        .arg("--config")
        .arg(config)
        .arg("execute")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Local dylib lookup is infrastructure, not content or a provider credential.
    if let Some(path) = std::env::var_os("DYLD_LIBRARY_PATH") {
        process.env("DYLD_LIBRARY_PATH", path);
    }
    let mut child = process.spawn().map_err(|_| "image host could not start")?;
    let operation = async {
        let mut input = child.stdin.take().ok_or("image host input unavailable")?;
        input
            .write_all(&bytes)
            .await
            .map_err(|_| "image host input failed")?;
        drop(input);
        let output = child.stdout.take().ok_or("image host output unavailable")?;
        let mut bytes = Vec::new();
        output
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "image host output failed")?;
        if bytes.len() > MAX_OUTPUT {
            return Err("image host response too large".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "image host status unavailable")?;
        if !status.success() {
            return Err(
                "image execution unavailable; original review and financial attempt retained"
                    .into(),
            );
        }
        serde_json::from_slice(&bytes).map_err(|_| "invalid image host response".into())
    };
    tokio::time::timeout(Duration::from_secs(150), operation)
        .await
        .map_err(|_| "image host response uncertain; recover the original review")?
}
impl SpaceTools<'_> {
    pub(super) fn image_stage(&self, call: &ToolCall) -> Result<String, String> {
        self.check_selection()?;
        if call.name != TOOL
            || !self
                .names
                .iter()
                .any(|(d, _, t)| d.name == TOOL && t == TOOL)
        {
            return Err("image tool no longer offered".into());
        }
        let input: Input = serde_json::from_value(call.arguments.clone())
            .map_err(|_| "invalid image arguments")?;
        let previous = retained_or_schedule(self.journal, &self.space, self.source, call)?;
        let retained: Pending =
            serde_json::from_str(&previous).map_err(|_| "invalid retained image scheduling")?;
        if retained.schema != "runtime-image/1"
            || retained.space != self.space
            || retained.source != self.source
        {
            return Err("retained image source differs".into());
        }
        if !matches!(retained.progress, Progress::CatchingUp) {
            return Ok(previous);
        }
        let agent = self
            .e
            .account()
            .ok_or("image agent unavailable")?
            .identity_id;
        let progress = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(execute(Command::Stage {
                agent,
                space: self.space.clone(),
                source: self.source.into(),
                prompt: input.prompt,
            }))
        })?;
        self.check_selection()?;
        let result = serde_json::to_string(&Pending {
            progress,
            ..retained
        })
        .map_err(|_| "invalid image progress")?;
        replace_step(self.journal, self.source, call, &previous, &result)?;
        Ok(result)
    }
}
fn retained_or_schedule(
    path: &Path,
    space: &str,
    source: &str,
    call: &ToolCall,
) -> Result<String, String> {
    let state = JournalState::load(path)?;
    if let Some(step) = state
        .steps
        .iter()
        .find(|(s, _)| s == source)
        .and_then(|(_, steps)| steps.iter().find(|s| s.call == *call))
    {
        return Ok(step.result.clone());
    }
    let initial = serde_json::to_string(&Pending {
        schema: "runtime-image/1".into(),
        space: space.into(),
        source: source.into(),
        progress: Progress::CatchingUp,
    })
    .map_err(|_| "invalid image scheduling")?;
    // Retain scheduling before subprocess IO. Lost stage replies recover through
    // the same source/input; this record is never an approval or financial fact.
    JournalState::update(path, |state| {
        if unresolved_count(&state.steps) >= JOURNAL_KEEP {
            return Err("image scheduling capacity reached; finish existing reviews first".into());
        }
        if let Some((_, steps)) = state.steps.iter_mut().find(|(s, _)| s == source) {
            if steps.iter().any(|s| s.call == *call) {
                return Err("image scheduling changed".into());
            }
            steps.push(ToolStep {
                call: call.clone(),
                result: initial.clone(),
                is_error: false,
            });
        } else {
            state.steps.push((
                source.into(),
                vec![ToolStep {
                    call: call.clone(),
                    result: initial.clone(),
                    is_error: false,
                }],
            ));
        }
        trim_completed(&mut state.steps);
        Ok(true)
    })?;
    Ok(initial)
}

pub(super) struct Worker(tokio::task::JoinHandle<()>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}
/// Independent of model turns: pending owner reviews must survive both source
/// cursor advancement and long provider latency without blocking chat replies.
pub(super) fn start(e: &Arc<Client>, cx: &Ctx) -> Worker {
    let (e, journal, agent, device) = (
        e.clone(),
        cx.journal.clone(),
        cx.me.clone(),
        cx.device.clone(),
    );
    Worker(tokio::spawn(async move {
        loop {
            if e.account()
                .is_none_or(|a| a.identity_id != agent || a.device_id != device)
            {
                return;
            }
            if configured() && e.agent_state().await.is_ok_and(|s| s.status == "active") {
                for space in e.spaces() {
                    advance_pending(&e, &journal, &agent, &device, &space.id).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }))
}
async fn advance_pending(e: &Arc<Client>, journal: &Path, agent: &str, device: &str, space: &str) {
    if !configured() {
        return;
    }
    let Ok(state) = JournalState::load(journal) else {
        return;
    };
    for (source, steps) in state.steps {
        for step in steps
            .into_iter()
            .filter(|s| s.call.name == TOOL && !s.is_error)
        {
            let Ok(mut pending) = serde_json::from_str::<Pending>(&step.result) else {
                continue;
            };
            if pending.schema != "runtime-image/1"
                || pending.space != space
                || pending.source != source
            {
                continue;
            }
            let command = match &pending.progress {
                Progress::Review {
                    request,
                    content_hash,
                } => {
                    if !e.requests().iter().any(|r| {
                        r.id == *request
                            && r.tool_approval.as_ref().is_some_and(|approval| approval.content_hash == *content_hash)
                            && r.status == client_core::RequestStatus::Approved
                    }) {
                        continue;
                    }
                    Command::Advance {
                        agent: agent.into(),
                        space: space.into(),
                        request: request.clone(),
                    }
                }
                Progress::CatchingUp => {
                    let Ok(input) = serde_json::from_value::<Input>(step.call.arguments.clone())
                    else {
                        continue;
                    };
                    Command::Stage {
                        agent: agent.into(),
                        space: space.into(),
                        source: source.clone(),
                        prompt: input.prompt,
                    }
                }
                _ => continue,
            };
            // Approval errors preserve the original locator; neither another
            // review nor a new provider attempt is inferred from a lost reply.
            let Ok(progress) = execute(command).await else {
                continue;
            };
            if progress == Progress::CatchingUp {
                continue;
            }
            if e.account()
                .is_none_or(|a| a.identity_id != agent || a.device_id != device)
            {
                return;
            }
            pending.progress = progress;
            let Ok(result) = serde_json::to_string(&pending) else {
                continue;
            };
            let _ = replace_step(journal, &source, &step.call, &step.result, &result);
        }
    }
}

fn replace_step(
    path: &Path,
    source: &str,
    call: &ToolCall,
    before: &str,
    after: &str,
) -> Result<(), String> {
    JournalState::update(path, |state| {
        let current = state
            .steps
            .iter_mut()
            .find(|(s, _)| s == source)
            .and_then(|(_, steps)| {
                steps
                    .iter_mut()
                    .find(|s| s.call == *call && s.result == before)
            })
            .ok_or("image scheduling changed")?;
        current.result = after.into();
        Ok(true)
    })
    .map(|_| ())
}
fn unresolved(step: &ToolStep) -> bool {
    step.call.name == TOOL
        && !step.is_error
        && serde_json::from_str::<Pending>(&step.result)
            .map_or(true, |p| !matches!(p.progress, Progress::Delivered { .. }))
}
fn unresolved_count(steps: &[(String, Vec<ToolStep>)]) -> usize {
    steps
        .iter()
        .flat_map(|(_, steps)| steps)
        .filter(|s| unresolved(s))
        .count()
}
/// Keep completed-source history bounded without evicting unresolved images.
/// Admission refuses at JOURNAL_KEEP unresolved calls before subprocess IO.
pub(super) fn trim_completed(steps: &mut Vec<(String, Vec<ToolStep>)>) {
    let completed = steps
        .iter()
        .filter(|(_, s)| !s.iter().any(unresolved))
        .count();
    let mut remove = completed.saturating_sub(JOURNAL_KEEP);
    steps.retain(|(_, s)| {
        if remove > 0 && !s.iter().any(unresolved) {
            remove -= 1;
            false
        } else {
            true
        }
    });
}
