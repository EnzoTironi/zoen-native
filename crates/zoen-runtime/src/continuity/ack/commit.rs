//! The original SQL-first commit and its acknowledged failure cuts.
use crate::continuity::{ManagedRuntime, Prepared};
use crate::{RuntimeAuthority, RuntimeError};
use sqlx::{Postgres, Transaction};
impl ManagedRuntime {
    pub(super) async fn commit_financial_ack(
        &mut self,
        runtime: &RuntimeAuthority,
        mut sql: Transaction<'static, Postgres>,
        tx: foundationdb::Transaction,
        prepared: &Prepared,
    ) -> Result<(), RuntimeError> {
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 130 {
            return Err(RuntimeError::Unavailable);
        }
        self.write_sql(&mut sql, prepared).await?;
        sql.commit().await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 131 {
            return Err(RuntimeError::Unavailable);
        }
        self.write_fdb(runtime, &tx, prepared).await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 132 {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(self.reconcile(runtime)).await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 133 {
            return Err(RuntimeError::Unavailable);
        }
        Ok(())
    }
}
