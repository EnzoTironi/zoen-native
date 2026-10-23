//! Debug diagnosis through the pinned upstream retry runner. Only closed counts
//! leave this module; conflicting keys, identities and payloads never do.
use super::*;
use foundationdb::{FdbResult, RunnerHooks, TransactionCommitError};

pub(super) struct Diagnostics {
    head: Vec<u8>,
    lease: Vec<u8>,
    commits: AtomicU64,
    head_conflicts: AtomicU64,
    lease_conflicts: AtomicU64,
    other_conflicts: AtomicU64,
    unreported: AtomicU64,
    closure_errors: AtomicU64,
    report_errors: AtomicU64,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Counts {
    pub commits: u64,
    pub head: u64,
    pub lease: u64,
    pub other: u64,
    pub unreported: u64,
    pub closure_errors: u64,
    pub report_errors: u64,
}
impl Diagnostics {
    pub fn new(cell: &Cell, space: &str) -> Self {
        Self {
            head: cell.space_key(space, "head"),
            lease: ownership::lease_key(&cell.root, ownership::partition_of(space)),
            commits: AtomicU64::new(0),
            head_conflicts: AtomicU64::new(0),
            lease_conflicts: AtomicU64::new(0),
            other_conflicts: AtomicU64::new(0),
            unreported: AtomicU64::new(0),
            closure_errors: AtomicU64::new(0),
            report_errors: AtomicU64::new(0),
        }
    }
    pub fn configure(trx: &Transaction) -> Result<(), FdbBindingError> {
        trx.set_option(foundationdb::options::TransactionOption::ReportConflictingKeys)?;
        Ok(())
    }
    pub fn counts(&self) -> Counts {
        Counts {
            commits: self.commits.load(Ordering::Relaxed),
            head: self.head_conflicts.load(Ordering::Relaxed),
            lease: self.lease_conflicts.load(Ordering::Relaxed),
            other: self.other_conflicts.load(Ordering::Relaxed),
            unreported: self.unreported.load(Ordering::Relaxed),
            closure_errors: self.closure_errors.load(Ordering::Relaxed),
            report_errors: self.report_errors.load(Ordering::Relaxed),
        }
    }
    fn classify(&self, begin: &[u8], end: &[u8]) {
        let head = begin <= self.head.as_slice() && self.head.as_slice() < end;
        let lease = begin <= self.lease.as_slice() && self.lease.as_slice() < end;
        if head {
            self.head_conflicts.fetch_add(1, Ordering::Relaxed);
        }
        if lease {
            self.lease_conflicts.fetch_add(1, Ordering::Relaxed);
        }
        if !head && !lease {
            self.other_conflicts.fetch_add(1, Ordering::Relaxed);
        }
    }
}
impl RunnerHooks for Diagnostics {
    fn on_closure_error(&self, _: &foundationdb::FdbError) {
        self.closure_errors.fetch_add(1, Ordering::Relaxed);
    }
    async fn on_commit_error(&self, error: &TransactionCommitError) -> FdbResult<()> {
        self.commits.fetch_add(1, Ordering::Relaxed);
        if error.code() == 1020 {
            // The upstream API reads the failed transaction BEFORE on_error
            // resets it. No replacement retry loop or timing policy lives here.
            let ranges = match tokio::time::timeout(
                std::time::Duration::from_millis(50),
                error.conflicting_keys(),
            )
            .await
            {
                Ok(Ok(ranges)) => ranges,
                _ => {
                    self.report_errors.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
            };
            if ranges.is_empty() {
                self.unreported.fetch_add(1, Ordering::Relaxed);
            }
            for range in ranges {
                self.classify(range.begin(), range.end());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classification_exposes_only_counts_and_respects_exclusive_end() {
        let diagnostic = Diagnostics {
            head: b"PRIVATE-HEAD-CANARY".to_vec(),
            lease: b"PRIVATE-LEASE-CANARY".to_vec(),
            commits: AtomicU64::new(0),
            head_conflicts: AtomicU64::new(0),
            lease_conflicts: AtomicU64::new(0),
            other_conflicts: AtomicU64::new(0),
            unreported: AtomicU64::new(0),
            closure_errors: AtomicU64::new(0),
            report_errors: AtomicU64::new(0),
        };
        diagnostic.classify(b"PRIVATE-HEAD-CANARY", b"PRIVATE-HEAD-CANARY\0");
        diagnostic.classify(b"PRIVATE-LEASE-CANARY", b"PRIVATE-LEASE-CANARY\0");
        diagnostic.classify(b"PRIVATE-OTHER-CANARY", b"PRIVATE-OTHER-CANARY\0");
        diagnostic.classify(b"A", b"PRIVATE-HEAD-CANARY");
        let counts = diagnostic.counts();
        assert_eq!(
            counts,
            Counts {
                commits: 0,
                head: 1,
                lease: 1,
                other: 2,
                unreported: 0,
                closure_errors: 0,
                report_errors: 0,
            }
        );
        assert_eq!(
            format!("{counts:?}"),
            "Counts { commits: 0, head: 1, lease: 1, other: 2, unreported: 0, closure_errors: 0, report_errors: 0 }"
        );
    }
}
