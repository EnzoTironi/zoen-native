//! Experiment analysis and acquisition funnels (ADR 0044). Inputs are the pseudonymous
//! exposure and activity rows (≤ 35 days); outputs are per-arm aggregates only.
//!
//! Per arm: units, messages per exposed day (CUPED-adjusted with the unit's messages in the
//! 7 days before exposure), D1 retention and crash-free sessions. Treatment vs control
//! reports the difference, a fixed-horizon p-value and an always-valid p-value from a
//! mixture sequential probability ratio test (mSPRT), so Enzo can look every day without
//! inflating false positives. Guardrails flag an arm that hurts retention or crash-free
//! sessions with always-valid p < 0.05.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use sqlx::PgPool;

use super::today;

/// Standard normal survival ×2 (two-sided p) via erfc (Abramowitz–Stegun 7.1.26).
pub fn two_sided_p(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    (poly * (-x * x).exp()).clamp(0.0, 1.0)
}

/// Always-valid p-value of a difference `d` with variance `v` (mSPRT, normal mixture with
/// variance `tau2`, Johari et al. 2017).
pub fn msprt_p(d: f64, v: f64, tau2: f64) -> f64 {
    if v <= 0.0 || tau2 <= 0.0 {
        return 1.0;
    }
    let lambda = (v / (v + tau2)).sqrt() * ((tau2 * d * d) / (2.0 * v * (v + tau2))).exp();
    (1.0 / lambda).min(1.0)
}

#[derive(Default, Clone, Debug)]
pub struct Unit {
    pub y: f64,
    pub x: f64,
    pub d1: Option<bool>,
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn var(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64
}

fn round(x: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (x * f).round() / f
}

/// Compares every arm with the first (control) on a continuous metric, CUPED-adjusted.
pub fn compare(arms: &BTreeMap<String, Vec<Unit>>, control: &str) -> BTreeMap<String, Value> {
    let all: Vec<&Unit> = arms.values().flatten().collect();
    let xs: Vec<f64> = all.iter().map(|u| u.x).collect();
    let ys: Vec<f64> = all.iter().map(|u| u.y).collect();
    let (mx, my) = (mean(&xs), mean(&ys));
    let vx = var(&xs);
    let cov = if xs.len() > 1 {
        xs.iter()
            .zip(&ys)
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>()
            / (xs.len() - 1) as f64
    } else {
        0.0
    };
    let theta = if vx > 0.0 { cov / vx } else { 0.0 };
    let adj = |u: &Unit| u.y - theta * (u.x - mx);
    // Mixture variance: effects about a fifth of a standard deviation are what we expect.
    let tau2 = (var(&ys) * 0.04).max(1e-9);
    let empty = Vec::new();
    let c = arms.get(control).unwrap_or(&empty);
    let cy: Vec<f64> = c.iter().map(adj).collect();
    let c_d1: Vec<bool> = c.iter().filter_map(|u| u.d1).collect();
    let mut out = BTreeMap::new();
    for (name, units) in arms {
        let ay: Vec<f64> = units.iter().map(adj).collect();
        let raw: Vec<f64> = units.iter().map(|u| u.y).collect();
        let d1: Vec<bool> = units.iter().filter_map(|u| u.d1).collect();
        let d1_rate =
            (!d1.is_empty()).then(|| d1.iter().filter(|b| **b).count() as f64 / d1.len() as f64);
        let mut v = json!({
            "units": units.len(),
            "messages_per_day": round(mean(&raw), 3),
            "messages_per_day_cuped": round(mean(&ay), 3),
            "retention_d1_pct": d1_rate.map(|r| round(r * 100.0, 1)),
            "retention_d1_units": d1.len(),
        });
        if name != control && !units.is_empty() && !c.is_empty() {
            let d = mean(&ay) - mean(&cy);
            let se2 = var(&ay) / ay.len() as f64 + var(&cy) / cy.len() as f64;
            let z = if se2 > 0.0 { d / se2.sqrt() } else { 0.0 };
            v["vs_control"] = json!({
                "messages_per_day_diff": round(d, 3),
                "messages_per_day_lift_pct": (mean(&cy) != 0.0).then(|| round(d / mean(&cy) * 100.0, 1)),
                "p_value": round(if se2 > 0.0 { two_sided_p(z) } else { 1.0 }, 4),
                "always_valid_p": round(msprt_p(d, se2, tau2), 4),
                "cuped_theta": round(theta, 3),
            });
            if let (Some(t), false) = (d1_rate, c_d1.is_empty()) {
                let cr = c_d1.iter().filter(|b| **b).count() as f64 / c_d1.len() as f64;
                let dv = t - cr;
                let vv = t * (1.0 - t) / d1.len() as f64 + cr * (1.0 - cr) / c_d1.len() as f64;
                v["vs_control"]["retention_d1_diff_pct"] = json!(round(dv * 100.0, 1));
                v["vs_control"]["retention_d1_always_valid_p"] =
                    json!(round(msprt_p(dv, vv, 0.05f64.powi(2)), 4));
            }
        }
        out.insert(name.clone(), v);
    }
    out
}

/// Every experiment with exposures in the window, arm by arm.
pub async fn experiments(
    pool: &PgPool,
    config: &roda_proto::experiments::RemoteConfig,
    everyone: bool,
    counters: &BTreeMap<String, i64>,
) -> anyhow::Result<Value> {
    let t = today();
    /// flag, variant, first day, messages since, messages the week before, D1, excluded.
    type ExposureRow = (String, String, i32, i64, i64, Option<bool>, bool);
    let rows: Vec<ExposureRow> = sqlx::query_as(
        "SELECT e.flag, e.variant, (e.first_day - DATE '1970-01-01')::int,
                coalesce(sum(a.sent) FILTER (WHERE a.day >= e.first_day), 0)::bigint,
                coalesce(sum(a.sent) FILTER (WHERE a.day < e.first_day AND a.day >= e.first_day - 7), 0)::bigint,
                bool_or(a.day = e.first_day + 1 AND (a.sent > 0 OR a.syncs > 0)),
                coalesce(bool_or(a.test), false) OR coalesce(bool_or(a.kind = 1), false)
         FROM metrics_exposures e LEFT JOIN metrics_activity a ON a.pid = e.pid
         GROUP BY e.pid, e.flag, e.variant, e.first_day",
    )
    .fetch_all(pool)
    .await?;
    let mut by_flag: BTreeMap<String, BTreeMap<String, Vec<Unit>>> = BTreeMap::new();
    for (flag, variant, first, y, x, d1, excluded) in rows {
        if excluded && !everyone {
            continue;
        }
        let days = (t - first + 1).max(1) as f64;
        by_flag
            .entry(flag)
            .or_default()
            .entry(variant)
            .or_default()
            .push(Unit {
                y: y as f64 / days,
                x: x as f64 / 7.0,
                // D1 is known once the day after exposure is over.
                d1: (first + 1 < t).then_some(d1.unwrap_or(false)),
            });
    }
    let mut out = serde_json::Map::new();
    for (flag, mut arms) in by_flag {
        let def = config.flags.get(&flag);
        let control = def
            .map(|f| f.variants[0].key.clone())
            .unwrap_or_else(|| "control".into());
        if let Some(f) = def {
            for v in &f.variants {
                arms.entry(v.key.clone()).or_default();
            }
        }
        let mut result = compare(&arms, &control);
        // Crash-free sessions per arm, from opt-in app health.
        for (arm, v) in result.iter_mut() {
            let s = counters
                .get(&format!("client_sessions:{flag}:{arm}"))
                .copied()
                .unwrap_or(0);
            let c = counters
                .get(&format!("client_crashes:{flag}:{arm}"))
                .copied()
                .unwrap_or(0);
            v["crash_free_sessions_pct"] =
                json!((s > 0).then(|| round((1.0 - c as f64 / s as f64) * 100.0, 2)));
            v["health_sessions"] = json!(s);
        }
        let guardrails: Vec<String> = def.map(|f| f.guardrails.clone()).unwrap_or_default();
        let mut hits = vec![];
        let ctrl_cf = result
            .get(&control)
            .and_then(|v| v["crash_free_sessions_pct"].as_f64());
        for (arm, v) in &result {
            if arm == &control {
                continue;
            }
            let vs = &v["vs_control"];
            if guardrails.iter().any(|g| g == "retention_d1")
                && vs["retention_d1_diff_pct"]
                    .as_f64()
                    .is_some_and(|d| d < 0.0)
                && vs["retention_d1_always_valid_p"]
                    .as_f64()
                    .is_some_and(|p| p < 0.05)
            {
                hits.push(format!("{arm}: D1 retention down"));
            }
            if guardrails.iter().any(|g| g == "crash_free") {
                if let (Some(a), Some(c)) = (v["crash_free_sessions_pct"].as_f64(), ctrl_cf) {
                    if a < c - 0.5 {
                        hits.push(format!("{arm}: crash-free sessions down {:.2} pts", c - a));
                    }
                }
            }
        }
        let decision = if !hits.is_empty() {
            "stop: guardrail hit"
        } else if result.iter().any(|(a, v)| {
            a != &control
                && v["vs_control"]["always_valid_p"]
                    .as_f64()
                    .is_some_and(|p| p < 0.05)
        }) {
            "significant: an arm differs on messages per day (always-valid p < 0.05)"
        } else {
            "keep running"
        };
        out.insert(
            flag.clone(),
            json!({
                "control": control,
                "arms": result,
                "guardrails": guardrails,
                "guardrail_hits": hits,
                "send_latency_note": "send latency is measured at the relay for everyone, not per arm",
                "decision": decision,
            }),
        );
    }
    Ok(Value::Object(out))
}

/// Signups, activation and D1/D7 retention by acquisition source, and by the arm of each
/// onboarding experiment.
pub async fn funnels(
    pool: &PgPool,
    config: &roda_proto::experiments::RemoteConfig,
    everyone: bool,
) -> anyhow::Result<(Value, Value)> {
    let t = today();
    const FUNNEL: &str = "
        count(*)::bigint,
        count(*) FILTER (WHERE m.first_message_at IS NOT NULL)::bigint,
        count(*) FILTER (WHERE m.first_message_at <= m.signup_at + interval '24 hours')::bigint,
        count(*) FILTER (WHERE s.d + 1 < $1)::bigint,
        count(*) FILTER (WHERE s.d + 1 < $1 AND EXISTS (
            SELECT 1 FROM metrics_activity a WHERE a.pid = m.pid AND a.day = DATE '1970-01-01' + s.d + 1
              AND (a.sent > 0 OR a.syncs > 0)))::bigint,
        count(*) FILTER (WHERE s.d + 7 < $1)::bigint,
        count(*) FILTER (WHERE s.d + 7 < $1 AND EXISTS (
            SELECT 1 FROM metrics_activity a WHERE a.pid = m.pid AND a.day = DATE '1970-01-01' + s.d + 7
              AND (a.sent > 0 OR a.syncs > 0)))::bigint";
    type Row = (String, i64, i64, i64, i64, i64, i64, i64);
    let shape = |rows: Vec<Row>| -> Value {
        let mut m = serde_json::Map::new();
        for (k, n, sent, a24, e1, r1, e7, r7) in rows {
            let pct = |a: i64, b: i64| (b > 0).then(|| round(a as f64 / b as f64 * 100.0, 1));
            m.insert(
                k,
                json!({
                    "signups": n,
                    "sent_first_message_pct": pct(sent, n),
                    "activation_24h_pct": pct(a24, n),
                    "retention_d1_pct": pct(r1, e1),
                    "retention_d7_pct": pct(r7, e7),
                }),
            );
        }
        Value::Object(m)
    };
    let by_source: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT coalesce(m.source, 'unknown') || coalesce(':' || m.campaign, ''), {FUNNEL}
         FROM metrics_accounts m,
              LATERAL (SELECT ((m.signup_at AT TIME ZONE 'UTC')::date - DATE '1970-01-01')::int AS d) s
         WHERE m.kind = 0 AND (NOT m.test OR $2)
         GROUP BY 1 ORDER BY 1"
    )))
    .bind(t)
    .bind(everyone)
    .fetch_all(pool)
    .await?;
    let onboarding: Vec<String> = config
        .onboarding
        .rules
        .iter()
        .filter_map(|r| r.experiment.clone())
        .collect();
    let by_arm: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT e.flag || ':' || e.variant, {FUNNEL}
         FROM metrics_accounts m JOIN metrics_exposures e ON e.pid = m.pid AND e.flag = ANY($3),
              LATERAL (SELECT ((m.signup_at AT TIME ZONE 'UTC')::date - DATE '1970-01-01')::int AS d) s
         WHERE m.kind = 0 AND (NOT m.test OR $2)
         GROUP BY 1 ORDER BY 1"
    )))
    .bind(t)
    .bind(everyone)
    .bind(&onboarding)
    .fetch_all(pool)
    .await?;
    Ok((shape(by_source), shape(by_arm)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_p_values() {
        assert!((two_sided_p(1.96) - 0.05).abs() < 0.001);
        assert!((two_sided_p(0.0) - 1.0).abs() < 1e-6);
        assert!(two_sided_p(5.0) < 1e-5);
    }

    #[test]
    fn msprt_is_conservative_but_finds_big_effects() {
        // Same z = 2 at fixed horizon is p≈0.045; always-valid p is larger.
        let v = 0.01;
        assert!(msprt_p(0.2, v, 0.01) > two_sided_p(2.0));
        assert!(msprt_p(1.0, v, 0.01) < 0.001);
        assert_eq!(msprt_p(0.0, v, 0.01), 1.0);
    }

    #[test]
    fn cuped_removes_pre_period_noise() {
        // y = x + effect; x varies a lot. CUPED should recover the effect with tiny variance.
        let mut arms = BTreeMap::new();
        for (arm, effect) in [("control", 0.0), ("direct", 1.0)] {
            let units = (0..200)
                .map(|i| {
                    let x = (i % 17) as f64 * 3.0;
                    Unit {
                        y: x + effect,
                        x,
                        d1: Some(i % 2 == 0),
                    }
                })
                .collect();
            arms.insert(arm.to_string(), units);
        }
        let r = compare(&arms, "control");
        let vs = &r["direct"]["vs_control"];
        assert!((vs["messages_per_day_diff"].as_f64().unwrap() - 1.0).abs() < 0.01);
        assert!(vs["always_valid_p"].as_f64().unwrap() < 0.001, "{vs}");
        assert_eq!(vs["retention_d1_diff_pct"].as_f64().unwrap(), 0.0);
    }
}
