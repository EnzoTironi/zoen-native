//! Existing watermark authority plus separately retained peer-clock evidence.
use crate::engine::{now_ms, Engine, R};
use protocol::{
    read_time::{ReadTime, MAX_MEMBERS, RETENTION_MS},
    EphemeralKind,
};
use types::SpaceKind;

impl Engine {
    pub(crate) fn read_time_allowed(&self, space: &str) -> R<bool> {
        let Some(state) = self.state.spaces.get(space) else {
            return Ok(false);
        };
        Ok(matches!(state.kind, SpaceKind::Direct | SpaceKind::Group)
            && self.receipt_members(space)?.len() <= MAX_MEMBERS)
    }
    /// A local read advance is the only source of a new local report timestamp.
    pub(crate) fn record_local_read_time(
        &self,
        space: &str,
        after: Option<u64>,
        through: u64,
    ) -> R<()> {
        if !self.read_time_allowed(space)? {
            return Ok(());
        }
        let Some(me) = self.me.as_deref() else {
            return Ok(());
        };
        let at = now_ms();
        if at <= 0 || at > 253_402_300_799_999 {
            return Ok(());
        }
        self.store.record_read_time(
            space,
            me,
            &store::ReadTimeRow {
                after_seq: after.unwrap_or(0),
                through_seq: through,
                reported_at_ms: at,
                observed_at_ms: at,
            },
            at.saturating_sub(RETENTION_MS),
        )?;
        Ok(())
    }
    pub(crate) fn read_time_announcement(&self, space: &str, seq: u64) -> R<EphemeralKind> {
        if self.read_time_allowed(space)? {
            if let Some(me) = self.me.as_deref() {
                if let Some(row) = self
                    .store
                    .read_times(space, me, now_ms().saturating_sub(RETENTION_MS))?
                    .into_iter()
                    .find(|r| r.through_seq == seq)
                {
                    return Ok(EphemeralKind::ReadTime(ReadTime {
                        after_seq: row.after_seq,
                        through_seq: seq,
                        reported_at_ms: row.reported_at_ms,
                    }));
                }
            }
        }
        Ok(EphemeralKind::Read { seq })
    }
    /// Caller has an authenticated relay identity. Neither presence nor local arrival
    /// time is allowed to supply the peer-reported clock.
    pub(crate) fn receive_read_time(
        &mut self,
        space: &str,
        from: &str,
        report: &ReadTime,
        observed_at_ms: i64,
    ) -> R<bool> {
        if !report.valid() || !self.receipt_scope(space, from)? {
            return Ok(false);
        }
        let raised = self.receipt(space, from, 0, report.through_seq)?;
        if !self.read_time_allowed(space)? {
            return Ok(raised);
        }
        let stored = self.store.record_read_time(
            space,
            from,
            &store::ReadTimeRow {
                after_seq: report.after_seq,
                through_seq: report.through_seq,
                reported_at_ms: report.reported_at_ms,
                observed_at_ms,
            },
            now_ms().saturating_sub(RETENTION_MS),
        )?;
        Ok(raised || stored)
    }
    pub(crate) fn receipts_with_times_to_replay(&mut self) -> R<Vec<(String, EphemeralKind)>> {
        let mut frames = self.receipts_to_replay();
        for space in &self.net.synced {
            if !self.read_time_allowed(space)? {
                continue;
            }
            let (Some(me), Some(through)) = (self.me.as_deref(), self.read_up_to(space)) else {
                continue;
            };
            for row in self
                .store
                .read_times(space, me, now_ms().saturating_sub(RETENTION_MS))?
            {
                if row.through_seq <= through {
                    frames.push((
                        space.clone(),
                        EphemeralKind::ReadTime(ReadTime {
                            after_seq: row.after_seq,
                            through_seq: row.through_seq,
                            reported_at_ms: row.reported_at_ms,
                        }),
                    ));
                }
            }
        }
        Ok(frames)
    }
}
