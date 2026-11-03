//! Time-derived state, card headlines, and portable snapshots.

use super::{clamp, hike_name, maptap_board, schedule, t, thousands, tr, ts, Value};
use serde_json::json;
use std::time::Duration;

#[cfg(test)]
mod tests;

/// A rollback contributes no elapsed time; `abs_diff` preserves the full signed range.
fn elapsed_millis(later: i64, earlier: i64) -> u64 {
    if later > earlier {
        later.abs_diff(earlier)
    } else {
        0
    }
}

// ── leitura (com o tempo passando, no caso do bichinho) ──

/// O estado como a interface deve ver agora (o bichinho sente fome com o tempo).
#[must_use]
pub fn view(app: &str, state: &Value, now: i64) -> Value {
    let mut v = state.clone();
    if app == "pet" && !state["released"].as_bool().unwrap_or(false) {
        let elapsed = elapsed_millis(now, state["updated"].as_i64().unwrap_or(now));
        let h = Duration::from_millis(elapsed).as_secs_f64() / 3_600.0;
        let asleep = state["asleep"].as_bool().unwrap_or(false);
        v["fullness"] = json!(clamp(state["fullness"].as_f64().unwrap_or(50.0) - 6.0 * h).round());
        v["energy"] = json!(clamp(
            state["energy"].as_f64().unwrap_or(50.0) + if asleep { 12.0 * h } else { -4.0 * h }
        )
        .round());
        v["joy"] = json!(clamp(state["joy"].as_f64().unwrap_or(50.0) - 5.0 * h).round());
        v["mood"] = json!(pet_mood(&v));
    }
    v
}

pub(super) fn pet_mood(v: &Value) -> &'static str {
    if v["released"].as_bool().unwrap_or(false) {
        return ts("Foi embora", "Gone");
    }
    if v["asleep"].as_bool().unwrap_or(false) {
        return ts("Dormindo", "Sleeping");
    }
    let (f, e, j) = (
        v["fullness"].as_f64().unwrap_or(0.0),
        v["energy"].as_f64().unwrap_or(0.0),
        v["joy"].as_f64().unwrap_or(0.0),
    );
    if f < 25.0 {
        ts("Com fome", "Hungry")
    } else if e < 25.0 {
        ts("Cansado", "Tired")
    } else if j < 30.0 {
        ts("Entediado", "Bored")
    } else if f > 85.0 && j > 85.0 {
        ts("Radiante", "Thrilled")
    } else {
        ts("Feliz aqui", "Happy here")
    }
}

#[must_use]
pub fn last_action(state: &Value) -> Option<String> {
    let first = state["log"].as_array()?.first()?;
    Some(format!(
        "{} {}",
        first["who"].as_str()?,
        first["what"].as_str()?
    ))
}

/// Approximate count for display ratios only. Exact counts remain in labels and
/// canonical JSON; these floats do not decide limits, money or authorization.
#[expect(
    clippy::cast_precision_loss,
    reason = "Card/snapshot completion fractions are approximate visual projections; exact counts and all decisions retain their integer source."
)]
fn visual_count(count: usize) -> f64 {
    count as f64
}

mod snapshots;
pub use snapshots::snapshot;

mod headlines;
pub use headlines::headline;
