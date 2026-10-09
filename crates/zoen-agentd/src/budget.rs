//! Each owner's daily allowance of sandbox time, per heavy tier. Charged per second of
//! running time; a suspended sandbox costs nothing (ADR 0028 §1).

use crate::router::Tier;
use serde::{Deserialize, Serialize};

const DAY_MS: i64 = 86_400_000;

/// Our cost estimates from ADR 0028 §9 (Hetzner-class metal, 60% use), in micro-dollars per
/// second: $0.0069/h and $0.0135/h.
pub const MICRO_USD_PER_SEC_MICROVM: f64 = 0.0069 / 3600.0 * 1e6;
pub const MICRO_USD_PER_SEC_BROWSER: f64 = 0.0135 / 3600.0 * 1e6;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DailyBudget {
    pub day: i64,
    pub microvm_secs_limit: u64,
    pub browser_secs_limit: u64,
    pub microvm_secs_used: u64,
    pub browser_secs_used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error("today's {tier:?} time is used up")]
    Exhausted { tier: Tier },
}

/// One charge, for the owner's usage view and `UsageRecorded`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Usage {
    pub tier: Tier,
    pub secs: u64,
    pub est_micro_usd: f64,
}

impl DailyBudget {
    pub fn new(microvm_secs_limit: u64, browser_secs_limit: u64, now_ms: i64) -> Self {
        DailyBudget {
            day: now_ms.div_euclid(DAY_MS),
            microvm_secs_limit,
            browser_secs_limit,
            microvm_secs_used: 0,
            browser_secs_used: 0,
        }
    }

    fn roll(&mut self, now_ms: i64) {
        let day = now_ms.div_euclid(DAY_MS);
        if day != self.day {
            self.day = day;
            self.microvm_secs_used = 0;
            self.browser_secs_used = 0;
        }
    }

    pub fn remaining(&mut self, tier: Tier, now_ms: i64) -> u64 {
        self.roll(now_ms);
        match tier {
            Tier::MicroVm => self
                .microvm_secs_limit
                .saturating_sub(self.microvm_secs_used),
            Tier::Browser => self
                .browser_secs_limit
                .saturating_sub(self.browser_secs_used),
            Tier::None | Tier::Wasm => u64::MAX,
        }
    }

    pub fn admit(&mut self, tier: Tier, now_ms: i64) -> Result<u64, BudgetError> {
        match self.remaining(tier, now_ms) {
            0 => Err(BudgetError::Exhausted { tier }),
            left => Ok(left),
        }
    }

    pub fn charge(&mut self, tier: Tier, secs: u64, now_ms: i64) -> Usage {
        self.roll(now_ms);
        let rate = match tier {
            Tier::MicroVm => {
                self.microvm_secs_used += secs;
                MICRO_USD_PER_SEC_MICROVM
            }
            Tier::Browser => {
                self.browser_secs_used += secs;
                MICRO_USD_PER_SEC_BROWSER
            }
            Tier::None | Tier::Wasm => 0.0,
        };
        Usage {
            tier,
            secs,
            est_micro_usd: rate * secs as f64,
        }
    }
}
