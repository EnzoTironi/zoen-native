//! A registered managed device cannot downgrade through ordinary publication.
use super::*;
use relay_admission::managed_images::ConsumePublish;

pub(super) enum Appended {
    Sequenced(Sequencing),
    /// Exact original evidence only; do not append, fan out or mint a permit.
    Observed(protocol::Sequenced),
}

impl Session {
    pub(super) async fn append_effect(
        &self,
        env: &protocol::Envelope,
        target_known: bool,
    ) -> Result<Appended, crate::log::Reject> {
        let Some(configured) = self.st.log.managed_device(&self.device) else {
            return self
                .st
                .log
                .append(env, target_known)
                .await
                .map(Appended::Sequenced);
        };
        let request = self
            .managed_binding
            .as_ref()
            .ok_or_else(|| crate::log::Reject::retry("managed runtime binding required"))?;
        crate::runtime_binding::expectation(
            &self.st,
            &self.identity,
            &self.device,
            request.clone(),
        )
        .await
        .map_err(crate::log::Reject::retry)?;
        let current = crate::runtime_binding::current_managed(&self.st, &configured, request)
            .await
            .map_err(crate::log::Reject::retry)?;
        let _generation = configured
            .upload_guard(&current)
            .map_err(crate::log::Reject::retry)?;
        let requested =
            crate::runtime_binding::checkpoint(request).map_err(crate::log::Reject::retry)?;
        match configured
            .anchor()
            .consume_publish(&requested, env)
            .map_err(|_| crate::log::Reject::retry("managed publication refused"))?
        {
            ConsumePublish::Fresh(permit) => self
                .st
                .log
                .append_managed_publish(env, target_known, permit)
                .await
                .map(Appended::Sequenced),
            ConsumePublish::Consumed(condition) => self
                .st
                .log
                .managed_publication_observation(&condition, env)
                .await
                .map_err(|_| {
                    crate::log::Reject::retry("original managed publication is uncertain")
                })?
                .map(Appended::Observed)
                .ok_or_else(|| {
                    crate::log::Reject::retry("original managed publication is uncertain")
                }),
        }
    }
}
