//! Classification cannot manufacture an authenticated report from an unavailable reply.
use super::{image_ipc, observation::Counts};
use crate::image_protocol::{PolicyOperation, PolicyProgress, PolicyUnavailable};
pub(super) enum Fault {
    Ipc(image_ipc::Failure),
    Runtime(
        PolicyUnavailable,
        Option<zoen_runtime::OwnerPolicyContextStage>,
    ),
    Mismatch,
}
pub(super) enum OptionsReply {
    Report(Box<types::owner_policy_context::OwnerPolicyOptions>),
    Refuse,
    Pending(Fault),
}
pub(super) fn options(
    reply: Result<PolicyProgress, image_ipc::Failure>,
    digest: &str,
) -> OptionsReply {
    match reply {
        Ok(PolicyProgress::Options { options }) if options.inquiry_digest == digest => {
            OptionsReply::Report(options)
        }
        Ok(PolicyProgress::OptionsRefused { inquiry_digest }) if inquiry_digest == digest => {
            OptionsReply::Refuse
        }
        Ok(PolicyProgress::Unavailable {
            operation: PolicyOperation::Context,
            code,
            context_stage,
        }) => OptionsReply::Pending(Fault::Runtime(code, context_stage)),
        Err(error) => OptionsReply::Pending(Fault::Ipc(error)),
        _ => OptionsReply::Pending(Fault::Mismatch),
    }
}
pub(super) fn observe(counts: &mut Counts, fault: Fault) {
    match fault {
        Fault::Ipc(error) => counts.ipc_error = Some(error),
        Fault::Runtime(error, stage) => {
            counts.runtime_error = Some(error);
            counts.runtime_stage = stage;
        }
        Fault::Mismatch => counts.mismatched += 1,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_stage_survives_without_becoming_an_authenticated_refusal(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let progress: PolicyProgress = serde_json::from_str(
            r#"{"status":"unavailable","operation":"context","code":"denied","context_stage":"admission"}"#,
        )?;
        let OptionsReply::Pending(fault) = options(Ok(progress), "digest") else {
            panic!("unavailable context became an authoritative outcome");
        };
        let mut counts = Counts::default();
        observe(&mut counts, fault);
        assert_eq!(counts.runtime_error, Some(PolicyUnavailable::Denied));
        assert_eq!(
            counts.runtime_stage,
            Some(zoen_runtime::OwnerPolicyContextStage::Admission)
        );
        assert_eq!(counts.report_accepted, 0);
        assert_eq!(counts.refusal_accepted, 0);
        assert!(serde_json::from_str::<PolicyProgress>(
            r#"{"status":"unavailable","operation":"context","code":"denied","context_stage":"private-token/path"}"#
        ).is_err());
        let legacy: PolicyProgress = serde_json::from_str(
            r#"{"status":"unavailable","operation":"context","code":"denied"}"#,
        )?;
        assert!(matches!(
            options(Ok(legacy), "digest"),
            OptionsReply::Pending(Fault::Runtime(PolicyUnavailable::Denied, None))
        ));
        Ok(())
    }
    #[test]
    fn unavailable_malformed_wrong_digest_and_wrong_operation_never_become_refusal_or_report() {
        let pending = [
            Err(image_ipc::Failure::Malformed),
            Err(image_ipc::Failure::NonzeroExit),
            Ok(PolicyProgress::Unavailable {
                operation: PolicyOperation::Context,
                code: PolicyUnavailable::Denied,
                context_stage: Some(zoen_runtime::OwnerPolicyContextStage::Admission),
            }),
            Ok(PolicyProgress::Unavailable {
                operation: PolicyOperation::Import,
                code: PolicyUnavailable::Unavailable,
                context_stage: None,
            }),
            Ok(PolicyProgress::OptionsRefused {
                inquiry_digest: "another-inquiry".into(),
            }),
            Ok(PolicyProgress::Installed {
                policy_digest: "digest".into(),
            }),
        ];
        for reply in pending {
            assert!(matches!(options(reply, "digest"), OptionsReply::Pending(_)));
        }
        assert!(matches!(
            options(
                Ok(PolicyProgress::OptionsRefused {
                    inquiry_digest: "digest".into()
                }),
                "digest"
            ),
            OptionsReply::Refuse
        ));
    }
}
