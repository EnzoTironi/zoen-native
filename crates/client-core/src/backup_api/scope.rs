//! Current native account and one nonqueued backup mutation on this Client.
use super::*;
impl Client {
    pub(super) fn backup_write_guard(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, CoreError> {
        self.backup_write.try_lock().map_err(|_| {
            invalid(t(
                "Um backup já está em andamento. Aguarde e tente novamente.",
                "A backup is already in progress. Wait and try again.",
            ))
        })
    }
    pub(super) fn check_backup_creds(&self, creds: &Creds) -> Result<(), CoreError> {
        let engine = self.lock();
        engine.native_surface_owner(&creds.identity, &creds.key.id())?;
        let account = engine
            .account()
            .ok_or_else(|| invalid("backup account unavailable"))?;
        if http_base(&account.relay_url) != creds.base {
            return Err(invalid("backup account changed"));
        }
        Ok(())
    }
}
