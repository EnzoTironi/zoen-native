//! Bounded local commands carry locators or owner signatures, never authority.
use serde::Deserialize;
use serde_json::{json, Value};
use zoen_runtime::{
    FinancialState, OwnerPolicyInstallation, ReplyHostProgress, ReplyReceiptProgress,
    ReplyRuntimeHost,
};
pub(super) const MAX_INPUT: usize = 16384;
pub(super) const MAX_OUTPUT: usize = 65536;
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    SearchHistory {
        run: String,
        arguments: Value,
    },
    Discover {
        space: String,
    },
    Advance {
        run: String,
    },
    Pending {
        after: Option<String>,
    },
    PolicyContext {
        signed: Box<types::owner_policy_context::SignedOwnerPolicyInquiry>,
    },
    PolicyImport {
        signed: Box<types::owner_budget::SignedOwnerPolicy>,
    },
}
pub(super) async fn execute(host: &ReplyRuntimeHost, command: Command) -> Result<Value, String> {
    match command {
        Command::SearchHistory { run, arguments } => {
            let page = host
                .search_history(&run, &arguments)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({"status":"history","page":page}))
        }
        Command::Discover { space } => {
            let p = host.discover(&space).await.map_err(|e| e.to_string())?;
            Ok(
                json!({"status":"discovered","caught_up":p.caught_up,"complete":p.discovery_complete,"runs":p.runs}),
            )
        }
        Command::Pending { after } => {
            let page = host
                .pending_page(after.as_deref())
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({"status":"pending","runs":page.runs,"next":page.next}))
        }
        Command::PolicyContext { signed } => {
            use crate::owner_policy_protocol::PolicyProgress;
            let digest = event_log::owner_policy_inquiry::digest(&signed.inquiry)
                .ok_or("invalid inquiry")?;
            let reply = match host.owner_policy_options(&signed).await {
                Ok(options) => PolicyProgress::Options {
                    options: Box::new(options),
                },
                Err(error) => crate::owner_policy_response::context_failure(
                    error,
                    digest,
                    host.configured_device(),
                ),
            };
            serde_json::to_value(reply).map_err(|_| "invalid policy response".into())
        }
        Command::PolicyImport { signed } => {
            let installation = host
                .import_owner_policy(*signed)
                .await
                .map_err(|e| e.to_string())?;
            Ok(match installation {
                OwnerPolicyInstallation::Installed(digest) => {
                    json!({"status":"policy_installed","digest":digest})
                }
                OwnerPolicyInstallation::Refused(digest) => {
                    json!({"status":"policy_refused","digest":digest})
                }
            })
        }
        Command::Advance { run } => Ok(progress(
            Box::pin(host.advance(&run))
                .await
                .map_err(|e| e.to_string())?,
        )),
    }
}
fn financial(value: Option<FinancialState>) -> Value {
    match value {
        None => Value::Null,
        Some(FinancialState::Reserved) => json!({"status":"reserved"}),
        Some(FinancialState::Claimed) => json!({"status":"claimed"}),
        Some(FinancialState::Released) => json!({"status":"released"}),
        Some(FinancialState::Settled { units }) => json!({"status":"settled","units":units}),
    }
}
fn receipt(delivery: ReplyReceiptProgress) -> Value {
    match delivery {
        ReplyReceiptProgress::Waiting => json!({"status":"waiting"}),
        ReplyReceiptProgress::Accepted { accepted, .. } => {
            json!({"status":"accepted","through":accepted})
        }
        ReplyReceiptProgress::CatchingUp { through, .. } => {
            json!({"status":"catching_up","through":through})
        }
        ReplyReceiptProgress::Refused { reason } => json!({"status":"refused","reason":reason}),
    }
}
pub(super) fn progress(p: ReplyHostProgress) -> Value {
    match p {
        ReplyHostProgress::CatchingUp {
            run,
            financial: state,
        } => json!({"status":"catching_up","run":run,"financial":financial(state)}),
        ReplyHostProgress::Held {
            run,
            audit,
            financial: state,
        } => json!({"status":"held","run":run,"audit":audit,"financial":financial(state)}),
        ReplyHostProgress::Review {
            run,
            request,
            content_hash,
            status,
            financial: state,
        } => {
            json!({"status":"review","run":run,"request":request,"content_hash":content_hash,"review_status":status,"financial":financial(Some(state))})
        }
        ReplyHostProgress::AwaitingRecovery {
            run,
            financial: state,
        } => json!({"status":"recovery","run":run,"financial":financial(Some(state))}),
        ReplyHostProgress::Output {
            run,
            output,
            delivery,
            financial: state,
            audit,
        } => {
            json!({"status":"output","run":run,"client_id":output.client_id,"message":output.message,"delivery":receipt(delivery),"financial":financial(Some(state)),"audit":audit})
        }
        ReplyHostProgress::Completed {
            run,
            output,
            delivery,
        } => {
            json!({"status":"completed","run":run,"client_id":output.client_id,"message":output.message,"delivery":receipt(delivery)})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_cannot_choose_execution_authority_or_replace_config() {
        for input in [
            json!({"command":"advance","run":"x","approved":true}),
            json!({"command":"advance","run":"x","agent":"another"}),
            json!({"command":"discover","space":"x","credential_key":"another"}),
        ] {
            assert!(serde_json::from_value::<Command>(input).is_err());
        }
        assert_eq!(financial(None), Value::Null);
        assert_ne!(financial(None), financial(Some(FinancialState::Released)));
    }
}
