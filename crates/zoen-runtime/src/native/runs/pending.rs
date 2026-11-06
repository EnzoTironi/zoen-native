//! Page through immutable wakes; an authenticated delivery receipt ends reply work.
use super::*;
use crate::ReplyWorkPage;
use foundationdb::{KeySelector, RangeOption};

impl Execution {
    pub(crate) async fn pending_reply_page(
        &self,
        limit: u16,
        after: Option<&str>,
        custody: &Custody,
    ) -> Result<ReplyWorkPage, RuntimeError> {
        if !(1..=64).contains(&limit) || after.is_some_and(|cursor| !id(cursor)) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.reply_source()?;
        let tx = self.transaction().await?;
        let prefix = self.root.subspace(&("native-wake/1",));
        let mut range = RangeOption::from(prefix.range());
        if let Some(after) = after {
            range.begin = KeySelector::first_greater_than(prefix.pack(&(after,)));
        }
        let rows = super::super::storage::bounded_rows(
            &tx,
            range,
            usize::from(limit) + 1,
            crate::custody::MAX_SEALED,
        )
        .await?;
        let more = rows.len() > usize::from(limit);
        let mut runs = Vec::new();
        let mut through = None;
        let scanned = rows.len().min(usize::from(limit));
        for (key, sealed) in rows.into_iter().take(usize::from(limit)) {
            let (wake_id,): (String,) = prefix
                .unpack(&key)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if !id(&wake_id) {
                return Err(RuntimeError::InvalidBinding);
            }
            let wake: Wake = custody.open(&format!("native-wake/1/{wake_id}"), None, &sealed)?;
            if wake.locator.wake != wake_id {
                return Err(RuntimeError::InvalidBinding);
            }
            let (record, digest) = self.reply_in(&tx, &wake.locator.run, custody).await?;
            if record.locator != wake.locator || digest != wake.record_digest {
                return Err(RuntimeError::InvalidBinding);
            }
            if Box::pin(self.completed_reply_in(&tx, &wake.locator.run, custody))
                .await?
                .is_none()
            {
                runs.push(wake.locator);
            }
            through = Some(wake_id);
        }
        Ok(ReplyWorkPage {
            runs,
            next: if more { through } else { None },
            scanned: u16::try_from(scanned).map_err(|_| RuntimeError::EvidenceTooLarge)?,
        })
    }

    pub(crate) async fn pending_reply_runs(
        &self,
        limit: u16,
        custody: &Custody,
    ) -> Result<Vec<ReplyRun>, RuntimeError> {
        if !(1..=64).contains(&limit) {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut runs = Vec::new();
        let mut after = None;
        let mut scanned = 0usize;
        loop {
            // The legacy convenience call also passes completed pages, so a
            // completed prefix never hides a later runnable wake.
            let page = self
                .pending_reply_page(64, after.as_deref(), custody)
                .await?;
            scanned = scanned
                .checked_add(usize::from(page.scanned))
                .ok_or(RuntimeError::EvidenceTooLarge)?;
            if scanned > 8192 {
                return Err(RuntimeError::EvidenceTooLarge);
            }
            runs.extend(page.runs);
            if runs.len() >= usize::from(limit) || page.next.is_none() {
                runs.truncate(usize::from(limit));
                return Ok(runs);
            }
            after = page.next;
        }
    }
}
