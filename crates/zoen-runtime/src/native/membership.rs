//! Current certified membership hints; full native authorization happens later.
use super::*;
impl NativeCustody {
    pub(crate) async fn conversation_page(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        after: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), RuntimeError> {
        let credential = self.credential(agent, device)?;
        let scope = runtime.production_output_scope()?;
        scope.check_device(agent, device)?;
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        let tx = runtime.execution.transaction().await?;
        scope.check_in(&runtime.execution, &tx).await?;
        runtime
            .execution
            .reply_source()?
            .membership_page_in(&tx, agent, after)
            .await
            .map_err(|error| match error {
                zoen_relay::log::fdb::RuntimeMembershipError::Invalid => {
                    RuntimeError::InvalidBinding
                }
                zoen_relay::log::fdb::RuntimeMembershipError::Unavailable => {
                    RuntimeError::Unavailable
                }
            })
    }
}
