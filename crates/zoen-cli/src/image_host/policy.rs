//! Local transport of authenticated owner controls, never a model tool.
use crate::image_protocol::{Command, PolicyProgress};
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
            Err(_) => Err("owner policy installation unavailable; original remains pending".into()),
        },
        Command::PolicyContext { signed, .. } => {
            let digest = event_log::owner_policy_inquiry::digest(&signed.inquiry)
                .ok_or("invalid owner policy inquiry")?;
            match host.owner_policy_options(&signed).await {
                Ok(options) if options.inquiry_digest == digest => Ok(PolicyProgress::Options {
                    options: Box::new(options),
                }),
                Ok(_) => Err("owner policy context differs".into()),
                Err(_) => Err("owner policy context unavailable; original remains pending".into()),
            }
        }
        _ => Err("typed owner policy command required".into()),
    }
}
