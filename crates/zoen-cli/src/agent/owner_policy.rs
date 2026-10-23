//! Independently reconciled typed controls. Ordinary text and model output never
//! enter this worker; Core authenticates actual confirmed encrypted originals.
use super::*;
use crate::image_protocol::{Command, PolicyProgress};

pub(super) struct Worker(tokio::task::JoinHandle<()>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(super) fn start(e: &Arc<Client>, cx: &Ctx) -> Worker {
    let (e, agent, device) = (e.clone(), cx.me.clone(), cx.device.clone());
    Worker(tokio::spawn(async move {
        loop {
            if e.account()
                .is_none_or(|a| a.identity_id != agent || a.device_id != device)
            {
                return;
            }
            if image_ipc::configured()
                && e.agent_state()
                    .await
                    .is_ok_and(|s| matches!(s.status.as_str(), "active" | "paused"))
            {
                for space in e.spaces() {
                    reconcile(&e, &agent, &space.id).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }))
}

async fn reconcile(e: &Client, agent: &str, space: &str) {
    // Core omits exact authenticated queued/confirmed receipts. A crash after
    // SQL installation but before reporting replays the original signed version
    // through Runtime; there is no second authority or independent cursor.
    if let Ok(queries) = e.owner_policy_inquiries(space, 0, 50) {
        for query in queries {
            let Ok(current) = e.owner_policy_inquiry(space, query.source()) else {
                continue;
            };
            if current.agent() != agent || current.digest() != query.digest() {
                continue;
            }
            let command = Command::PolicyContext {
                agent: agent.into(),
                signed: Box::new(current.signed().clone()),
            };
            match image_ipc::execute::<PolicyProgress>(command).await {
                Ok(PolicyProgress::Options { options })
                    if options.inquiry_digest == current.digest() =>
                {
                    let _ = e.report_owner_policy_options(&current, *options);
                }
                Ok(PolicyProgress::OptionsRefused { inquiry_digest })
                    if inquiry_digest == current.digest() =>
                {
                    let _ = e.refuse_owner_policy_options(&current);
                }
                // Unavailable, malformed, mismatched and ambiguous responses
                // leave the original pending. Never manufacture refusal.
                _ => {}
            }
        }
    }
    if let Ok(submissions) = e.owner_policy_submissions(space, 0, 50) {
        for submission in submissions {
            let Ok(current) = e.owner_policy_submission(space, submission.source()) else {
                continue;
            };
            if current.agent() != agent || current.digest() != submission.digest() {
                continue;
            }
            let command = Command::PolicyImport {
                agent: agent.into(),
                signed: Box::new(current.signed().clone()),
                policy_digest: current.digest().into(),
            };
            match image_ipc::execute::<PolicyProgress>(command).await {
                Ok(PolicyProgress::Installed { policy_digest })
                    if policy_digest == current.digest() =>
                {
                    let _ = e.report_owner_policy(&current, Some(&policy_digest));
                }
                Ok(PolicyProgress::Refused { policy_digest })
                    if policy_digest == current.digest() =>
                {
                    let _ = e.report_owner_policy(&current, None);
                }
                _ => {}
            }
        }
    }
}
