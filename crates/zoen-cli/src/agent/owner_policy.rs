//! Independently reconciled typed controls. Ordinary text and model output never
//! enter this worker; Core authenticates actual confirmed encrypted originals.
use super::*;
use crate::image_protocol::{Command, PolicyOperation, PolicyProgress};
mod observation;
mod reply;
use observation::{Agent, CoreStage, Counts, Observer, Summary};

pub(super) struct Worker(tokio::task::JoinHandle<()>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(super) fn start(e: &Arc<Client>, cx: &Ctx) -> Worker {
    let (e, agent, device) = (e.clone(), cx.me.clone(), cx.device.clone());
    Worker(tokio::spawn(async move {
        let mut observer = Observer::default();
        observer.write(
            Summary::new(image_ipc::configured(), Agent::Starting),
            &mut std::io::stdout().lock(),
        );
        loop {
            if e.account()
                .is_none_or(|a| a.identity_id != agent || a.device_id != device)
            {
                return;
            }
            let configured = image_ipc::configured();
            let status = if !configured {
                Agent::Unavailable
            } else {
                match e.agent_state().await {
                    Ok(state) if matches!(state.status.as_str(), "active" | "paused") => {
                        Agent::Ready
                    }
                    Ok(_) => Agent::Inactive,
                    Err(_) => Agent::Unavailable,
                }
            };
            let mut summary = Summary::new(configured, status);
            if status == Agent::Ready {
                for space in e.spaces() {
                    reconcile(&e, &agent, &space.id, &mut summary).await;
                }
            }
            observer.write(summary, &mut std::io::stdout().lock());
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }))
}
fn scan_error(counts: &mut Counts, error: &client_core::CoreError) {
    // Non-private/non-owned spaces legitimately fail discovery. Distinguish that
    // from candidates rejected inside a valid private DM and from store failures.
    if matches!(error, client_core::CoreError::Forbidden { .. }) {
        counts.scope_rejected += 1;
    } else {
        counts.core_error = Some(observation::failure(CoreStage::Scan, error));
    }
}
async fn reconcile(e: &Client, agent: &str, space: &str, summary: &mut Summary) {
    // Core omits exact authenticated queued/confirmed receipts. A crash after
    // installation replays the original through Runtime, never a second authority.
    match e.scan_owner_policy_inquiries(space, 0, 50) {
        Ok(scan) => {
            let counts = &mut summary.inquiries;
            counts.candidates = counts.candidates.saturating_add(scan.counts.candidates);
            counts.rejected = counts.rejected.saturating_add(scan.counts.rejected);
            counts.already_reported = counts
                .already_reported
                .saturating_add(scan.counts.already_reported);
            counts.eligible += scan.inquiries.len();
            for query in scan.inquiries {
                let current = match e.owner_policy_inquiry(space, query.source()) {
                    Ok(current) => current,
                    Err(error) => {
                        counts.core_error = Some(observation::failure(CoreStage::Recheck, &error));
                        continue;
                    }
                };
                if current.agent() != agent || current.digest() != query.digest() {
                    counts.mismatched += 1;
                    continue;
                }
                counts.rechecked += 1;
                let command = Command::PolicyContext {
                    agent: agent.into(),
                    signed: Box::new(current.signed().clone()),
                };
                counts.ipc_calls += 1;
                match reply::options(image_ipc::execute_observed(command).await, current.digest()) {
                    reply::OptionsReply::Report(options) => {
                        match e.report_owner_policy_options(&current, *options) {
                            Ok(()) => counts.report_accepted += 1,
                            Err(error) => {
                                counts.core_error =
                                    Some(observation::failure(CoreStage::Report, &error))
                            }
                        }
                    }
                    reply::OptionsReply::Refuse => match e.refuse_owner_policy_options(&current) {
                        Ok(()) => counts.refusal_accepted += 1,
                        Err(error) => {
                            counts.core_error =
                                Some(observation::failure(CoreStage::Report, &error))
                        }
                    },
                    reply::OptionsReply::Pending(fault) => reply::observe(counts, fault),
                }
            }
        }
        Err(error) => scan_error(&mut summary.inquiries, &error),
    }
    match e.owner_policy_submissions(space, 0, 50) {
        Ok(submissions) => {
            let counts = &mut summary.submissions;
            counts.eligible += submissions.len();
            for submission in submissions {
                let current = match e.owner_policy_submission(space, submission.source()) {
                    Ok(current) => current,
                    Err(error) => {
                        counts.core_error = Some(observation::failure(CoreStage::Recheck, &error));
                        continue;
                    }
                };
                if current.agent() != agent || current.digest() != submission.digest() {
                    counts.mismatched += 1;
                    continue;
                }
                counts.rechecked += 1;
                let command = Command::PolicyImport {
                    agent: agent.into(),
                    signed: Box::new(current.signed().clone()),
                    policy_digest: current.digest().into(),
                };
                counts.ipc_calls += 1;
                let result = match image_ipc::execute_observed::<PolicyProgress>(command).await {
                    Ok(PolicyProgress::Installed { policy_digest })
                        if policy_digest == current.digest() =>
                    {
                        Some((e.report_owner_policy(&current, Some(&policy_digest)), true))
                    }
                    Ok(PolicyProgress::Refused { policy_digest })
                        if policy_digest == current.digest() =>
                    {
                        Some((e.report_owner_policy(&current, None), false))
                    }
                    Ok(PolicyProgress::Unavailable {
                        operation: PolicyOperation::Import,
                        code,
                    }) => {
                        counts.runtime_error = Some(code);
                        None
                    }
                    Err(error) => {
                        counts.ipc_error = Some(error);
                        None
                    }
                    _ => {
                        counts.mismatched += 1;
                        None
                    }
                };
                if let Some((result, installed)) = result {
                    match result {
                        Ok(()) if installed => counts.report_accepted += 1,
                        Ok(()) => counts.refusal_accepted += 1,
                        Err(error) => {
                            counts.core_error =
                                Some(observation::failure(CoreStage::Report, &error))
                        }
                    }
                }
            }
        }
        Err(error) => scan_error(&mut summary.submissions, &error),
    }
}
