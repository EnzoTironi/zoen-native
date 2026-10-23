//! Existing controlled scope fixtures, compiled only in tests.
use super::*;
impl Execution {
    // Fixture core projections are compiled only in tests. They do not implement
    // MLS sender, ordered grants, approvals or certified device-state import.
    #[cfg(test)]
    pub async fn seed(&self, step: &VerifiedStep, custody: &Custody) -> Result<(), RuntimeError> {
        let step = step.fixture();
        let trx = self.transaction().await?;
        let expires = trx
            .get_read_version()
            .await?
            .checked_add(60_000_000)
            .ok_or(RuntimeError::Unavailable)?;
        let context = &step.request.context;
        for (key, fence) in [
            (self.run(&context.run_id, "lease"), &step.run_fence),
            (self.device(context), &step.device_fence),
        ] {
            trx.set(
                &key,
                &serde_json::to_vec(&Lease {
                    holder: fence.holder.clone(),
                    token: fence.token,
                    expires,
                })
                .unwrap(),
            );
        }
        let capsule = CoreCapsule {
            request: step.request.clone(),
            frontier: step.frontier.clone(),
            policy_digest: step.policy_digest.clone(),
        };
        let (_, sealed) = custody.seal(&format!("core/{}", context.run_id), &capsule)?;
        trx.set(&self.run(&context.run_id, "core"), &sealed);
        trx.set(
            &self.root.pack(&("frontier", context.space.as_str())),
            step.frontier.as_bytes(),
        );
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        Ok(())
    }
    #[cfg(test)]
    pub async fn has_admission(&self, attempt: &str) -> bool {
        self.transaction()
            .await
            .unwrap()
            .get(&self.attempt(attempt, "admitted"), false)
            .await
            .unwrap()
            .is_some()
    }
    #[cfg(test)]
    pub async fn take_device(&self, step: &VerifiedStep) {
        let step = step.fixture();
        let trx = self.transaction().await.unwrap();
        let expires = trx.get_read_version().await.unwrap() + 60_000_000;
        trx.set(
            &self.device(&step.request.context),
            &serde_json::to_vec(&Lease {
                holder: "new-owner".into(),
                token: step.device_fence.token + 1,
                expires,
            })
            .unwrap(),
        );
        trx.commit().await.unwrap();
    }
}
