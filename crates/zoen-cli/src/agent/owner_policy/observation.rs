//! Closed, deduplicated operational evidence. No locators, payload, Debug or Display.
use super::super::image_ipc;
use crate::image_protocol::PolicyUnavailable;
use client_core::CoreError;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Agent {
    Starting,
    Ready,
    Unavailable,
    Inactive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Core {
    Forbidden,
    Invalid,
    NotFound,
    Stale,
    Storage,
    DraftRejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CoreStage {
    Scan,
    Recheck,
    Report,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct CoreFailure {
    stage: CoreStage,
    code: Core,
}
pub(super) fn failure(stage: CoreStage, error: &CoreError) -> CoreFailure {
    CoreFailure {
        stage,
        code: core(error),
    }
}
pub(super) fn core(error: &CoreError) -> Core {
    match error {
        CoreError::Forbidden { .. } => Core::Forbidden,
        CoreError::Invalid { .. } => Core::Invalid,
        CoreError::NotFound { .. } => Core::NotFound,
        CoreError::Stale { .. } => Core::Stale,
        CoreError::Storage { .. } => Core::Storage,
        CoreError::DraftRejected { .. } => Core::DraftRejected,
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct Counts {
    pub scope_rejected: usize,
    pub candidates: u64,
    pub rejected: u64,
    pub already_reported: u64,
    pub eligible: usize,
    pub rechecked: usize,
    /// Calls into the transport, not spawned children or provider/financial attempts.
    pub ipc_calls: usize,
    pub mismatched: usize,
    pub report_accepted: usize,
    pub refusal_accepted: usize,
    pub core_error: Option<CoreFailure>,
    pub ipc_error: Option<image_ipc::Failure>,
    pub runtime_error: Option<PolicyUnavailable>,
    pub runtime_stage: Option<zoen_runtime::OwnerPolicyContextStage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Summary {
    pub configured: bool,
    pub agent: Agent,
    pub inquiries: Counts,
    pub submissions: Counts,
}
impl Summary {
    pub(super) fn new(configured: bool, agent: Agent) -> Self {
        Self {
            configured,
            agent,
            inquiries: Counts::default(),
            submissions: Counts::default(),
        }
    }
}
#[derive(Default)]
pub(super) struct Observer {
    previous: Option<Summary>,
}
impl Observer {
    pub(super) fn write(&mut self, summary: Summary, writer: &mut impl std::io::Write) {
        if self.previous.as_ref() == Some(&summary) {
            return;
        }
        // Serialize only this closed record. No caller-supplied message enters it.
        let record = serde_json::json!({"event":"zoen.owner-policy.worker.v1","summary":summary});
        if let Ok(mut bytes) = serde_json::to_vec(&record) {
            if bytes.len() < 2048 {
                bytes.push(b'\n');
                if writer.write_all(&bytes).is_ok() {
                    self.previous = Some(summary);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_observation_is_quiet_but_new_stage_error_is_emitted_without_private_error_text() {
        let private = "secret-token/private-config/account-id";
        let mut summary = Summary::new(true, Agent::Ready);
        summary.inquiries.candidates = 1;
        summary.inquiries.eligible = 1;
        summary.inquiries.core_error = Some(failure(
            CoreStage::Report,
            &CoreError::Storage {
                message: private.into(),
            },
        ));
        let mut observer = Observer::default();
        let mut output = Vec::new();
        observer.write(summary.clone(), &mut output);
        observer.write(summary.clone(), &mut output);
        assert_eq!(output.iter().filter(|b| **b == b'\n').count(), 1);
        summary.inquiries.core_error = None;
        summary.inquiries.ipc_error = Some(image_ipc::Failure::NonzeroExit);
        observer.write(summary, &mut output);
        assert_eq!(output.iter().filter(|b| **b == b'\n').count(), 2);
        let text = String::from_utf8(output).unwrap();
        assert!(!text.contains(private));
        assert!(text.contains("storage"));
        assert!(text.contains("nonzero_exit"));
        for line in text.lines() {
            assert!(line.len() < 2048);
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }
}
