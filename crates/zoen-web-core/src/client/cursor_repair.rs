//! Scheduling is ephemeral; only the existing verified logs supply durable cursors.
use super::{Client, ClientFrame, Cursor, Duration, Serialize, Signer, Update};
use protocol::cursor_repair::CursorRepairPolicy;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CursorRepairConfig {
    pub version: u32,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub initial_inactivity_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CursorRepairStatus {
    pub admission_ms: Option<u64>,
    pub abort_deadline_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CursorRepairTick {
    Update { update: Update },
    Expired,
}

impl<S: Signer> Client<S> {
    pub(super) fn validate_repair_clock(now: Duration) -> Result<(), String> {
        // Same integral-millisecond domain as the browser boundary. Keeping
        // representable deadlines here also protects direct Rust callers.
        if now.as_millis() > 281_474_976_710_655 || !now.subsec_nanos().is_multiple_of(1_000_000) {
            return Err("invalid browser reconciliation clock".into());
        }
        Ok(())
    }

    /// # Errors
    /// Returns an error if a configured repair duration exceeds the browser millisecond domain.
    pub fn cursor_repair_config(&self) -> Result<CursorRepairConfig, String> {
        let policy = CursorRepairPolicy::DEFAULT;
        Ok(CursorRepairConfig {
            version: 1,
            interval_ms: milliseconds(policy.interval)?,
            timeout_ms: milliseconds(policy.timeout)?,
            initial_inactivity_ms: milliseconds(policy.initial_inactivity)?,
        })
    }

    pub(super) fn note_repair_clock(&mut self, now: Duration) -> Result<(), String> {
        Self::validate_repair_clock(now)?;
        if now < self.repair_now {
            return Err("browser reconciliation clock moved backwards".into());
        }
        self.repair_now = now;
        Ok(())
    }

    pub(super) fn repair_submitted(&mut self) -> Result<(), String> {
        if self
            .cursor_repair
            .as_mut()
            .is_some_and(|repair| !repair.submitted())
        {
            return Err("too many outstanding sync requests".into());
        }
        Ok(())
    }

    pub(super) fn cursor_repair_status(&self) -> Result<Option<CursorRepairStatus>, String> {
        self.cursor_repair
            .as_ref()
            .map(|repair| {
                Ok(CursorRepairStatus {
                    admission_ms: repair.admission().map(milliseconds).transpose()?,
                    abort_deadline_ms: repair.abort_deadline().map(milliseconds).transpose()?,
                })
            })
            .transpose()
    }

    /// Called independently of UI/MLS maintenance, using the host monotonic clock.
    /// An already-expired dispatcher returns no Update and no outgoing effect.
    ///
    /// # Errors
    /// Returns an error for invalid or backwards clock values, missing bootstrap,
    /// too many outstanding requests, or failure to produce the client update.
    pub fn reconcile(&mut self, now: Duration) -> Result<CursorRepairTick, String> {
        self.note_repair_clock(now)?;
        let repair = self
            .cursor_repair
            .as_mut()
            .ok_or("cursor reconciliation needs bootstrap")?;
        if repair.abort_deadline().is_some_and(|until| now >= until) {
            return Ok(CursorRepairTick::Expired);
        }
        let mut frames = Vec::new();
        if repair.admission().is_some_and(|due| now >= due) {
            if !repair.begin(now) {
                return Err("too many outstanding sync requests".into());
            }
            let cursors = self
                .logs
                .iter()
                .map(|(space, log)| Cursor {
                    space: space.clone(),
                    next_seq: log.next_seq(),
                })
                .collect();
            frames.push(ClientFrame::Sync { cursors, all: true }.encode());
        }
        Ok(CursorRepairTick::Update {
            update: self.update(frames)?,
        })
    }
}

fn milliseconds(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_millis())
        .map_err(|_| "reconciliation duration exceeds browser millisecond domain".into())
}

#[cfg(test)]
mod tests {
    use super::{milliseconds, Duration};

    #[test]
    fn exported_milliseconds_are_checked_without_wrapping() -> Result<(), String> {
        assert_eq!(milliseconds(Duration::from_millis(60_001))?, 60_001);
        assert_eq!(milliseconds(Duration::from_millis(u64::MAX))?, u64::MAX);
        assert!(milliseconds(Duration::MAX).is_err());
        Ok(())
    }
}
