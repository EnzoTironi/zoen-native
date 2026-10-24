//! Classification cannot manufacture an authenticated report from an unavailable reply.
use super::{image_ipc, observation::Counts};
use crate::image_protocol::{PolicyOperation, PolicyProgress, PolicyUnavailable};
pub(super) enum Fault {
    Ipc(image_ipc::Failure),
    Runtime(PolicyUnavailable),
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
        }) => OptionsReply::Pending(Fault::Runtime(code)),
        Err(error) => OptionsReply::Pending(Fault::Ipc(error)),
        _ => OptionsReply::Pending(Fault::Mismatch),
    }
}
pub(super) fn observe(counts: &mut Counts, fault: Fault) {
    match fault {
        Fault::Ipc(error) => counts.ipc_error = Some(error),
        Fault::Runtime(error) => counts.runtime_error = Some(error),
        Fault::Mismatch => counts.mismatched += 1,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_malformed_wrong_digest_and_wrong_operation_never_become_refusal_or_report() {
        let pending = [
            Err(image_ipc::Failure::Malformed),
            Err(image_ipc::Failure::NonzeroExit),
            Ok(PolicyProgress::Unavailable {
                operation: PolicyOperation::Context,
                code: PolicyUnavailable::Denied,
            }),
            Ok(PolicyProgress::Unavailable {
                operation: PolicyOperation::Import,
                code: PolicyUnavailable::Unavailable,
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
