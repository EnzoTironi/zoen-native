//! Existing financial-reference keys, not another ledger or execution right.
use crate::continuity::{Prepared, SubjectUpdate};
use crate::finance::Finance;
use crate::{RuntimeAuthority, RuntimeError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
const PREFIX: &str = "financial_ack_v1_";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) attempt: String,
    pub(super) kind: String,
    pub(super) digest: String,
}
impl Record {
    pub(super) fn key(&self, runtime: &RuntimeAuthority) -> Vec<u8> {
        runtime.execution.root.pack(&(
            "financial",
            self.attempt.as_str(),
            self.kind.as_str(),
            self.digest.as_str(),
        ))
    }
    pub(super) fn subject(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-capture/1", &self.attempt, &self.digest))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn validate(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        if self.attempt.is_empty()
            || !matches!(self.kind.as_str(), "evidence" | "settle")
            || self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            || self.key(runtime).len() > 2048
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(super) async fn observe(
        &self,
        runtime: &RuntimeAuthority,
        sql: &mut Transaction<'_, Postgres>,
        tx: &foundationdb::Transaction,
    ) -> Result<crate::Binding, RuntimeError> {
        self.validate(runtime)?;
        let (binding, sealed_digest, nonce) =
            super::super::capture::reference(runtime, tx, &self.attempt, &self.digest).await?;
        Finance::observe_witness(sql, &binding, &nonce).await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runtime_financial_outbox o JOIN runtime_evidence e USING(attempt,digest) WHERE o.attempt=$1 AND o.kind=$2 AND o.digest=$3 AND encode(sha256(e.sealed),'hex')=$4 AND ($2='evidence' OR EXISTS(SELECT 1 FROM runtime_dispositions d WHERE d.attempt=o.attempt AND d.kind='settle' AND d.source_digest=o.digest)))",
        ).bind(&self.attempt).bind(&self.kind).bind(&self.digest).bind(sealed_digest)
            .fetch_one(&mut **sql).await?;
        if !valid {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(binding)
    }
    pub(super) async fn acknowledged(
        &self,
        sql: &mut Transaction<'_, Postgres>,
    ) -> Result<bool, RuntimeError> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_financial_acks WHERE attempt=$1 AND kind=$2 AND digest=$3)")
            .bind(&self.attempt).bind(&self.kind).bind(&self.digest).fetch_one(&mut **sql).await?)
    }
    pub(super) async fn stage(
        &self,
        runtime: &RuntimeAuthority,
        tx: &foundationdb::Transaction,
    ) -> Result<(), RuntimeError> {
        let key = self.key(runtime);
        match tx.get(&key, false).await? {
            Some(bytes) if bytes.as_ref() != b"1" => return Err(RuntimeError::RestoreUnreconciled),
            Some(_) => {}
            None => tx.set(&key, b"1"),
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub(super) record: Record,
    pub(super) financial: SubjectUpdate,
    pub(super) capture: SubjectUpdate,
}
impl Intent {
    pub(super) fn updates(&self) -> [SubjectUpdate; 2] {
        [self.financial.clone(), self.capture.clone()]
    }
    pub(super) fn effect(&self) -> Result<String, RuntimeError> {
        Ok(crate::hash(
            &serde_json::to_vec(&("runtime-financial-ack/1", self))
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
    }
    pub(super) fn seal(&self, runtime: &RuntimeAuthority) -> Result<String, RuntimeError> {
        let (_, bytes) = runtime.custody.seal("runtime-financial-ack/1", self)?;
        let value = format!("{PREFIX}{}", STANDARD.encode(bytes));
        if value.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(value)
    }
    pub(super) fn decode(
        runtime: &RuntimeAuthority,
        prepared: &Prepared,
    ) -> Result<Self, RuntimeError> {
        let value = prepared
            .intent
            .as_deref()
            .and_then(|v| v.strip_prefix(PREFIX))
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if value.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let bytes = STANDARD
            .decode(value)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let intent: Self = runtime
            .custody
            .open("runtime-financial-ack/1", None, &bytes)?;
        intent.record.validate(runtime)?;
        if intent.capture.subject != intent.record.subject()? || intent.capture.previous.is_none() {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if prepared.retirement
            || intent.effect()? != prepared.effect_digest
            || prepared.subject_updates != intent.updates()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(intent)
    }
}
pub(in crate::continuity) fn ack_intent(prepared: &Prepared) -> bool {
    prepared
        .intent
        .as_deref()
        .is_some_and(|v| v.starts_with(PREFIX))
}
