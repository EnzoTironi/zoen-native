//! Storage-independent GCRA arithmetic. The host owns the clock and atomic state.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Accepted { next_ns: u64 },
    Rejected { wait_ns: u64 },
}

/// Exactly the native limiter's saturating `UInt64` calculation. Rejected requests
/// do not update `tat_ns`. Native callers use elapsed monotonic time; durable
/// hosts must explicitly choose and persist their own nondecreasing clock basis.
#[must_use]
pub fn decide(
    now_ns: u64,
    tat_ns: Option<u64>,
    cost: u32,
    interval_ns: u64,
    burst: u32,
) -> Decision {
    let window = interval_ns.saturating_mul(u64::from(burst));
    let tat = tat_ns.unwrap_or(now_ns).max(now_ns);
    let next = tat.saturating_add(interval_ns.saturating_mul(u64::from(cost)));
    if next - now_ns > window {
        Decision::Rejected {
            wait_ns: next - now_ns - window,
        }
    } else {
        Decision::Accepted { next_ns: next }
    }
}
