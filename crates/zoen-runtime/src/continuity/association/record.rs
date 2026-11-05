//! Encrypted original correspondence over existing run/result rows.
use crate::continuity::{Prepared, SubjectUpdate};
use crate::finance::Finance;
use crate::{RuntimeAuthority, RuntimeError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
const PREFIX: &str = "association_v1_";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub(super) attempt: String,
    pub(super) digest: String,
    pub(super) association: String,
    pub(super) update: SubjectUpdate,
}
impl Intent {
    pub(super) fn subject(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-capture/1", &self.attempt, &self.digest))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        if self.association.len() > 24 * 1024 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        STANDARD
            .decode(&self.association)
            .map_err(|_| RuntimeError::InvalidBinding)
    }
    pub(super) fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-capture-association/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime
            .custody
            .seal("runtime-capture-association/1", self)?;
        let encoded = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(encoded)
    }
    pub(super) fn decode(
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<Self, RuntimeError> {
        let encoded = prepared
            .intent
            .as_deref()
            .and_then(|v| v.strip_prefix(PREFIX))
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if encoded.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let intent: Self = runtime
            .custody
            .open("runtime-capture-association/1", None, &bytes)?;
        if prepared.retirement
            || prepared.effect_digest != intent.effect()?
            || prepared.subject_updates != [intent.update.clone()]
            || intent.update.subject != intent.subject()?
            || intent.update.previous.is_none()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(intent)
    }
    pub(super) async fn stage(
        &self,
        runtime: &RuntimeAuthority,
        sql: &mut Transaction<'_, Postgres>,
        tx: &foundationdb::Transaction,
    ) -> Result<(), RuntimeError> {
        let (binding, _, nonce) =
            super::super::capture::reference(runtime, tx, &self.attempt, &self.digest).await?;
        Finance::observe_witness(sql, &binding, &nonce).await?;
        let bytes = self.bytes()?;
        runtime
            .execution
            .validate_capture_association_in(
                tx,
                &binding,
                (&self.digest, &nonce),
                &bytes,
                &runtime.custody,
            )
            .await?;
        let key = runtime.execution.attempt(&self.attempt, "native-capture/1");
        if let Some(prior) = tx.get(&key, false).await? {
            if prior.as_ref() != bytes.as_slice() {
                return Err(RuntimeError::RestoreUnreconciled);
            }
        } else {
            tx.set(&key, &bytes);
        }
        Ok(())
    }
}
pub(in crate::continuity) fn association_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|value| value.starts_with(PREFIX))
}
