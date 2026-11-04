//! Shared SQL financial phases. Recovery retains originals and never rebuilds
//! a fresh one-shot claim, provider capability, bill or release proof.
mod recovery;
use super::*;
use crate::finance::FreshClaim;
use crate::{Binding, VerifiedStep};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
const PREFIX: &str = "reserve_v1_";
const CLAIM_PREFIX: &str = "claim_v1_";
#[derive(Clone, Copy)]
enum Phase {
    Reserve,
    Claim,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    binding: Binding,
    previous: Option<String>,
    next: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claim_nonce: Option<String>,
}
impl Intent {
    fn domain(&self) -> &'static str {
        if self.claim_nonce.is_some() {
            "runtime-financial-claim/1"
        } else {
            "runtime-reservation/1"
        }
    }
    fn prefix(&self) -> &'static str {
        if self.claim_nonce.is_some() {
            CLAIM_PREFIX
        } else {
            PREFIX
        }
    }
    fn decode(runtime: &RuntimeAuthority, prepared: &Prepared) -> Result<Self, RuntimeError> {
        let original = prepared
            .intent
            .as_deref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let (prefix, domain) = if original.starts_with(CLAIM_PREFIX) {
            (CLAIM_PREFIX, "runtime-financial-claim/1")
        } else {
            (PREFIX, "runtime-reservation/1")
        };
        let encoded = original
            .strip_prefix(prefix)
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let intent: Self = runtime.custody.open(domain, None, &bytes)?;
        if intent.prefix() != prefix
            || intent.claim_nonce.as_ref().is_some_and(|nonce| {
                nonce.len() != 64
                    || !nonce
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(intent)
    }
    fn update(&self) -> Result<SubjectUpdate, RuntimeError> {
        Ok(SubjectUpdate {
            subject: crate::finance::snapshot::subject(
                &self.binding.context.owner,
                self.binding.period_start,
            )?,
            previous: self.previous.clone(),
            next: self.next.clone(),
        })
    }
    fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&(self.domain(), self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal(self.domain(), self)?;
        let encoded = format!("{}{}", self.prefix(), STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
}
pub(super) fn financial_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|value| value.starts_with(PREFIX) || value.starts_with(CLAIM_PREFIX))
}
async fn scope(
    runtime: &RuntimeAuthority,
    step: &VerifiedStep,
    binding: &Binding,
    tx: &foundationdb::Transaction,
) -> Result<(), RuntimeError> {
    let dispatch = runtime
        .gateway
        .preflight(step.request())
        .map_err(|_| RuntimeError::InvalidProfile)?;
    if step.binding(&dispatch, &runtime.price)? != *binding {
        return Err(RuntimeError::InvalidBinding);
    }
    let VerifiedStep::Native(native) = step else {
        return Err(RuntimeError::CoreAuthorityUnavailable);
    };
    native
        .scope_in(&runtime.execution, tx, &runtime.custody)
        .await
}
fn projected(
    before: &BTreeMap<String, String>,
    intent: &Intent,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let update = intent.update()?;
    if before.get(&update.subject) != update.previous.as_ref() {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let mut after = before.clone();
    after.insert(update.subject, update.next);
    Ok(after)
}
impl RuntimeAuthority {
    pub(crate) async fn reserve_verified_budget(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
    ) -> Result<(), RuntimeError> {
        self.financial_phase(step, binding, Phase::Reserve)
            .await
            .map(|_| ())
    }
    pub(crate) async fn claim_verified_budget(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
    ) -> Result<Option<FreshClaim>, RuntimeError> {
        self.financial_phase(step, binding, Phase::Claim).await
    }
    async fn financial_phase(
        &self,
        step: &VerifiedStep,
        binding: &Binding,
        phase: Phase,
    ) -> Result<Option<FreshClaim>, RuntimeError> {
        let Some(managed) = &self.runtime_continuity else {
            return match phase {
                Phase::Reserve => {
                    self.finance.reserve(binding).await?;
                    Ok(None)
                }
                Phase::Claim => self.finance.claim(binding).await,
            };
        };
        let mut managed = managed.lock().await;
        Box::pin(managed.reconcile(self)).await?;
        let mut sql = self.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = self.execution.transaction().await?;
        let before = inventory(self, &mut sql, &tx).await?;
        if managed.fdb_checkpoint(self, &tx).await? != previous
            || managed.anchor.reader().checkpoint().map_err(error)? != previous
            || managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(error)?
                != before
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        scope(self, step, binding, &tx).await?;
        let (inserted, outcome) = match phase {
            Phase::Reserve => (Finance::reserve_in(&mut sql, binding).await?, None),
            Phase::Claim => {
                let claim = Finance::claim_in(&mut sql, binding).await?;
                (claim.is_some(), claim)
            }
        };
        Finance::check_financial_deadline(&mut sql, binding, step.grant_deadline()).await?;
        if !inserted {
            sql.commit().await?;
            return Ok(outcome);
        }
        let after = inventory(self, &mut sql, &tx).await?;
        let subject =
            crate::finance::snapshot::subject(&binding.context.owner, binding.period_start)?;
        let intent = Intent {
            claim_nonce: outcome
                .as_ref()
                .map(|claim| claim.checkpoint_reference().nonce),
            binding: binding.clone(),
            previous: before.get(&subject).cloned(),
            next: after
                .get(&subject)
                .cloned()
                .ok_or(RuntimeError::InvalidBinding)?,
        };
        if projected(&before, &intent)? != after {
            return Err(RuntimeError::InvalidBinding);
        }
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &[intent.update()?],
                Some(&intent.seal(self)?),
            )
            .map_err(error)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst)
            == match phase {
                Phase::Reserve => 102,
                Phase::Claim => 106,
            }
        {
            return Err(RuntimeError::Unavailable);
        }
        // The original native/source read conflicts with an advancing source in
        // the subsequent FDB commit. SQL can retain a hold, never authorize HTTP.
        scope(self, step, binding, &tx).await?;
        Finance::check_financial_deadline(&mut sql, binding, step.grant_deadline()).await?;
        managed.write_sql(&mut sql, &prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst)
            == match phase {
                Phase::Reserve => 103,
                Phase::Claim => 107,
            }
        {
            return Err(RuntimeError::Unavailable);
        }
        managed.write_fdb(self, &tx, &prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst)
            == match phase {
                Phase::Reserve => 104,
                Phase::Claim => 108,
            }
        {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(self)).await?;
        #[cfg(test)]
        if self.fault.load(std::sync::atomic::Ordering::SeqCst)
            == match phase {
                Phase::Reserve => 105,
                Phase::Claim => 109,
            }
        {
            return Err(RuntimeError::Unavailable);
        }
        Ok(outcome)
    }
}

#[cfg(test)]
impl RuntimeAuthority {
    pub(crate) fn assert_legacy_reservation_format(binding: &Binding) -> Result<(), RuntimeError> {
        #[derive(Serialize)]
        struct Legacy<'a> {
            binding: &'a Binding,
            previous: Option<String>,
            next: String,
        }
        let legacy = Legacy {
            binding,
            previous: Some("0".repeat(64)),
            next: "1".repeat(64),
        };
        let current = Intent {
            binding: binding.clone(),
            previous: legacy.previous.clone(),
            next: legacy.next.clone(),
            claim_nonce: None,
        };
        let old = serde_json::to_vec(&legacy).map_err(|_| RuntimeError::InvalidBinding)?;
        let new = serde_json::to_vec(&current).map_err(|_| RuntimeError::InvalidBinding)?;
        assert!(
            old == new,
            "reservation intent bytes remain compatible when no claim is present"
        );
        let decoded: Intent =
            serde_json::from_slice(&old).map_err(|_| RuntimeError::InvalidBinding)?;
        assert_eq!(decoded.domain(), "runtime-reservation/1");
        assert_eq!(decoded.prefix(), PREFIX);
        assert_eq!(
            decoded.effect()?,
            crate::hash(
                &serde_json::to_vec(&("runtime-reservation/1", legacy))
                    .map_err(|_| RuntimeError::InvalidBinding)?
            )
        );
        Ok(())
    }
}
