//! Scoped discovery never installs another account's asynchronous lookup result.
use super::*;
impl Client {
    pub(crate) async fn find_member_candidates(
        &self,
        space: &str,
        query: &str,
        identity: &str,
        device: &str,
        owned_agents_only: bool,
    ) -> Result<Vec<crate::SpaceMemberContext>, CoreError> {
        let handle = query.trim().trim_start_matches('@').to_lowercase();
        self.member_candidates_lookup_mode(
            space,
            query,
            identity,
            device,
            owned_agents_only,
            async {
                match self
                    .request(protocol::Op::Lookup {
                        handle,
                        prefix: true,
                    })
                    .await?
                {
                    protocol::Reply::Profiles(profiles) => Ok(profiles),
                    _ => Err(unavailable()),
                }
            },
        )
        .await
    }

    /// The transport future is a port; both production and regressions cross the
    /// same before/after owner checks and the same durable profile installation.
    #[cfg(test)]
    pub(crate) async fn member_candidates_lookup(
        &self,
        space: &str,
        query: &str,
        identity: &str,
        device: &str,
        lookup: impl std::future::Future<Output = Result<Vec<types::Identity>, CoreError>>,
    ) -> Result<Vec<crate::SpaceMemberContext>, CoreError> {
        self.member_candidates_lookup_mode(space, query, identity, device, false, lookup)
            .await
    }

    async fn member_candidates_lookup_mode(
        &self,
        space: &str,
        query: &str,
        identity: &str,
        device: &str,
        owned_agents_only: bool,
        lookup: impl std::future::Future<Output = Result<Vec<types::Identity>, CoreError>>,
    ) -> Result<Vec<crate::SpaceMemberContext>, CoreError> {
        if query.len() > 256 {
            return Err(unavailable());
        }
        let local = self.lock().member_candidates_mode(
            space,
            query,
            identity,
            device,
            owned_agents_only,
        )?;
        if query.trim().trim_start_matches('@').is_empty() {
            return Ok(local);
        }
        let profiles = lookup.await?;
        let mut engine = self.lock();
        engine.member_candidates_mode(space, query, identity, device, owned_agents_only)?;
        engine.put_profiles(profiles)?;
        engine.member_candidates_mode(space, query, identity, device, owned_agents_only)
    }
}
fn unavailable() -> CoreError {
    CoreError::Forbidden {
        reason: crate::i18n::t(
            "Não foi possível buscar participantes.",
            "Member discovery unavailable.",
        ),
    }
}
