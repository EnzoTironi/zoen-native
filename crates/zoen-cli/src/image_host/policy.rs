//! Local transport of authenticated owner controls, never a model tool.
use crate::image_protocol::{Command, PolicyOperation, PolicyProgress, PolicyUnavailable};
use zoen_runtime::{ImageRuntimeHost, OwnerPolicyInstallation};

pub(super) async fn execute(
    host: &ImageRuntimeHost,
    command: Command,
) -> Result<PolicyProgress, String> {
    match command {
        Command::PolicyImport {
            signed,
            policy_digest,
            ..
        } => match host.import_owner_policy(*signed).await {
            Ok(OwnerPolicyInstallation::Installed(actual)) if actual == policy_digest => {
                Ok(PolicyProgress::Installed {
                    policy_digest: actual,
                })
            }
            Ok(OwnerPolicyInstallation::Refused(actual)) if actual == policy_digest => {
                Ok(PolicyProgress::Refused {
                    policy_digest: actual,
                })
            }
            Ok(_) => Err("owner policy installation response differs".into()),
            Err(error) => Ok(unavailable(PolicyOperation::Import, error)),
        },
        Command::PolicyContext { signed, .. } => {
            let digest = event_log::owner_policy_inquiry::digest(&signed.inquiry)
                .ok_or("invalid owner policy inquiry")?;
            match host.owner_policy_options(&signed).await {
                Ok(options) if options.inquiry_digest == digest => Ok(PolicyProgress::Options {
                    options: Box::new(options),
                }),
                Ok(_) => Err("owner policy context differs".into()),
                Err(error) => Ok(context_failure(error, digest, host.configured_device())),
            }
        }
        _ => Err("typed owner policy command required".into()),
    }
}

fn context_failure(
    error: zoen_runtime::OwnerPolicyContextFailure,
    inquiry_digest: String,
    configured_device: &str,
) -> PolicyProgress {
    if error.stage == zoen_runtime::OwnerPolicyContextStage::AdmissionMlsGroup
        && error.error == zoen_runtime::RuntimeError::Denied
    {
        return PolicyProgress::NeedsWelcome {
            inquiry_digest,
            device: configured_device.into(),
        };
    }
    PolicyProgress::Unavailable {
        operation: PolicyOperation::Context,
        code: category(error.error),
        context_stage: Some(error.stage),
    }
}

fn category(error: zoen_runtime::RuntimeError) -> PolicyUnavailable {
    use zoen_runtime::RuntimeError as E;
    match error {
        E::InvalidPolicy => PolicyUnavailable::InvalidPolicy,
        E::InvalidProfile => PolicyUnavailable::InvalidProfile,
        E::DeploymentMismatch => PolicyUnavailable::DeploymentMismatch,
        E::InvalidBinding => PolicyUnavailable::InvalidBinding,
        E::Denied => PolicyUnavailable::Denied,
        E::OverBudget => PolicyUnavailable::OverBudget,
        E::CoreAuthorityUnavailable => PolicyUnavailable::CoreAuthorityUnavailable,
        E::RestoreUnreconciled => PolicyUnavailable::RestoreUnreconciled,
        E::Unavailable => PolicyUnavailable::Unavailable,
        E::EvidenceTooLarge => PolicyUnavailable::EvidenceTooLarge,
    }
}

fn unavailable(operation: PolicyOperation, error: zoen_runtime::RuntimeError) -> PolicyProgress {
    PolicyProgress::Unavailable {
        operation,
        code: category(error),
        context_stage: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_real_missing_group_denial_can_request_a_configured_welcome() {
        use zoen_runtime::{
            OwnerPolicyContextFailure as Failure, OwnerPolicyContextStage as S, RuntimeError as E,
        };
        let device = "ab".repeat(32);
        for (stage, error) in [
            (S::AdmissionMlsGroup, E::Denied),
            (S::AdmissionMlsGroup, E::Unavailable),
            (S::AdmissionMlsPending, E::Denied),
            (S::AdmissionMlsStorage, E::Denied),
            (S::AdmissionMlsCredential, E::Denied),
            (S::AdmissionFreshness, E::Denied),
            (S::AdmissionMembership, E::Denied),
            (S::Source, E::Denied),
        ] {
            let response =
                context_failure(Failure { stage, error }, "exact-inquiry".into(), &device);
            if stage == S::AdmissionMlsGroup && error == E::Denied {
                assert!(
                    matches!(response, PolicyProgress::NeedsWelcome { inquiry_digest, device: actual }
                    if inquiry_digest == "exact-inquiry" && actual == device)
                );
            } else {
                assert!(matches!(response, PolicyProgress::Unavailable { .. }));
            }
        }
    }
    #[test]
    fn actual_runtime_denial_is_unavailable_not_an_installed_or_refused_receipt() {
        let progress = unavailable(PolicyOperation::Context, zoen_runtime::RuntimeError::Denied);
        let encoded = serde_json::to_value(progress).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({"status":"unavailable","operation":"context","code":"denied"})
        );
        let progress = unavailable(
            PolicyOperation::Import,
            zoen_runtime::RuntimeError::Unavailable,
        );
        assert!(matches!(
            progress,
            PolicyProgress::Unavailable {
                operation: PolicyOperation::Import,
                code: PolicyUnavailable::Unavailable,
                context_stage: None,
            }
        ));
    }
}
