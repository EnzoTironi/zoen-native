//! The numbers (definitions in `docs/product/metricas.md`). A day's [`DayStats`] are computed
//! from the pseudonymous rows while they exist and kept in `metrics_daily` once the day is
//! closed; the [`Report`] reads closed days from there and today live.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::{clock::today, RETENTION_DAYS};

/// A numerator over a denominator, so days can be summed before dividing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Ratio {
    pub num: i64,
    pub den: i64,
}

impl Ratio {
    pub fn value(&self) -> Option<f64> {
        (self.den > 0).then(|| self.num as f64 / self.den as f64)
    }
    fn add(&mut self, o: Ratio) {
        self.num += o.num;
        self.den += o.den;
    }
}

/// One UTC day, one population (people without QA accounts, or everyone).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DayStats {
    pub day: String,
    /// Active = sent ≥ 1 message or completed a sync. `_senders` = sent ≥ 1 (strict).
    pub dau: i64,
    pub wau: i64,
    pub mau: i64,
    pub dau_senders: i64,
    pub wau_senders: i64,
    pub mau_senders: i64,
    pub messages_day: i64,
    pub messages_week: i64,
    pub messages_month: i64,
    pub sessions: i64,
    pub signups: i64,
    pub agent_messages_day: i64,
    pub agent_messages_month: i64,
    pub agents_active_month: i64,
    /// Signups of the day before who sent a message within 24 h.
    pub activation_24h: Ratio,
    /// Signups of 7 days before who wrote to ≥ 2 different people in their first week.
    pub activation_2_people_week1: Ratio,
    /// Signups of 1, 7 and 30 days before who were active today.
    pub retention_d1: Ratio,
    pub retention_d7: Ratio,
    pub retention_d30: Ratio,
    pub spaces_active: i64,
    pub spaces_active_avg_members: Option<f64>,
    /// Global counters of the day (invites, Spaces created by kind, approvals, blobs,
    /// failed sends, latency buckets).
    pub counters: BTreeMap<String, i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Rolled {
    pub people: DayStats,
    pub everyone: DayStats,
}

fn pct(r: Ratio) -> Option<f64> {
    r.value().map(|v| (v * 1000.0).round() / 10.0)
}

fn div(a: i64, b: i64) -> Option<f64> {
    (b > 0).then(|| ((a as f64 / b as f64) * 100.0).round() / 100.0)
}

/// Test-handle prefixes, for the directory side of the queries.
fn prefixes() -> Vec<String> {
    std::env::var("ZOEN_METRICS_TEST_HANDLES")
        .unwrap_or_else(|_| super::DEFAULT_TEST_PREFIXES.into())
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub async fn day_stats(pool: &PgPool, d: i32, everyone: bool) -> anyhow::Result<DayStats> {
    let mut s = DayStats {
        day: sqlx::query_scalar("SELECT to_char(DATE '1970-01-01' + $1, 'YYYY-MM-DD')")
            .bind(d)
            .fetch_one(pool)
            .await?,
        ..DayStats::default()
    };
    // People's activity over the 30-day window ending today.
    let (dau, wau, mau, dau_s, wau_s, mau_s, m_d, m_w, m_m): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT
               count(DISTINCT pid) FILTER (WHERE day = D AND (sent > 0 OR syncs > 0)),
               count(DISTINCT pid) FILTER (WHERE day > D - 7 AND (sent > 0 OR syncs > 0)),
               count(DISTINCT pid) FILTER (WHERE sent > 0 OR syncs > 0),
               count(DISTINCT pid) FILTER (WHERE day = D AND sent > 0),
               count(DISTINCT pid) FILTER (WHERE day > D - 7 AND sent > 0),
               count(DISTINCT pid) FILTER (WHERE sent > 0),
               coalesce(sum(sent) FILTER (WHERE day = D), 0)::bigint,
               coalesce(sum(sent) FILTER (WHERE day > D - 7), 0)::bigint,
               coalesce(sum(sent), 0)::bigint
             FROM metrics_activity, (SELECT DATE '1970-01-01' + $1 AS D) w
             WHERE day BETWEEN D - 29 AND D AND kind = 0 AND (NOT test OR $2)",
    )
    .bind(d)
    .bind(everyone)
    .fetch_one(pool)
    .await?;
    (
        s.dau,
        s.wau,
        s.mau,
        s.dau_senders,
        s.wau_senders,
        s.mau_senders,
    ) = (dau, wau, mau, dau_s, wau_s, mau_s);
    (s.messages_day, s.messages_week, s.messages_month) = (m_d, m_w, m_m);

    let (sessions, agent_d, agent_m, agents): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           coalesce(sum(sessions) FILTER (WHERE day = D AND kind = 0), 0)::bigint,
           coalesce(sum(sent) FILTER (WHERE day = D AND kind = 1), 0)::bigint,
           coalesce(sum(sent) FILTER (WHERE kind = 1), 0)::bigint,
           count(DISTINCT pid) FILTER (WHERE kind = 1 AND sent > 0)
         FROM metrics_activity, (SELECT DATE '1970-01-01' + $1 AS D) w
         WHERE day BETWEEN D - 29 AND D AND (NOT test OR $2)",
    )
    .bind(d)
    .bind(everyone)
    .fetch_one(pool)
    .await?;
    (
        s.sessions,
        s.agent_messages_day,
        s.agent_messages_month,
        s.agents_active_month,
    ) = (sessions, agent_d, agent_m, agents);

    s.signups = sqlx::query_scalar(
        "SELECT count(*) FROM identities
         WHERE kind = 'Person' AND (created_at AT TIME ZONE 'UTC')::date = DATE '1970-01-01' + $1
           AND ($2 OR NOT EXISTS (SELECT 1 FROM unnest($3::text[]) p
                                  WHERE starts_with(lower(coalesce(handle, '')), p)))",
    )
    .bind(d)
    .bind(everyone)
    .bind(prefixes())
    .fetch_one(pool)
    .await?;

    // Cohorts: accounts by signup day (only accounts younger than the window have one).
    let cohort = |c: i32| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM metrics_accounts
             WHERE kind = 0 AND (NOT test OR $2)
               AND (signup_at AT TIME ZONE 'UTC')::date = DATE '1970-01-01' + $1",
        )
        .bind(c)
        .bind(everyone)
        .fetch_one(pool)
        .await
    };
    let active_from = |c: i32| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(DISTINCT pid) FROM metrics_activity
             WHERE day = DATE '1970-01-01' + $1 AND cohort = DATE '1970-01-01' + $2
               AND kind = 0 AND (NOT test OR $3) AND (sent > 0 OR syncs > 0)",
        )
        .bind(d)
        .bind(c)
        .bind(everyone)
        .fetch_one(pool)
        .await
    };
    for (days, slot) in [
        (1, &mut s.retention_d1),
        (7, &mut s.retention_d7),
        (30, &mut s.retention_d30),
    ] {
        let den = cohort(d - days).await?;
        if den > 0 {
            *slot = Ratio {
                num: active_from(d - days).await?,
                den,
            };
        }
    }
    let c1 = d - 1;
    let den = cohort(c1).await?;
    if den > 0 {
        let num: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM metrics_accounts
             WHERE kind = 0 AND (NOT test OR $2)
               AND (signup_at AT TIME ZONE 'UTC')::date = DATE '1970-01-01' + $1
               AND first_message_at <= signup_at + interval '24 hours'",
        )
        .bind(c1)
        .bind(everyone)
        .fetch_one(pool)
        .await?;
        s.activation_24h = Ratio { num, den };
    }
    let c7 = d - 7;
    let den = cohort(c7).await?;
    if den > 0 {
        let num: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM (
               SELECT pid FROM metrics_activity
               WHERE cohort = DATE '1970-01-01' + $1
                 AND day BETWEEN DATE '1970-01-01' + $1 AND DATE '1970-01-01' + $1 + 6
                 AND kind = 0 AND (NOT test OR $2)
               GROUP BY pid HAVING bit_count(bit_or(peers)::bit(64)) >= 2) x",
        )
        .bind(c7)
        .bind(everyone)
        .fetch_one(pool)
        .await?;
        s.activation_2_people_week1 = Ratio { num, den };
    }

    let (spaces, avg): (i64, Option<f64>) = sqlx::query_as(
        "SELECT count(*), avg(members)::float8 FROM metrics_spaces
         WHERE day = DATE '1970-01-01' + $1 AND messages > 0",
    )
    .bind(d)
    .fetch_one(pool)
    .await?;
    s.spaces_active = spaces;
    s.spaces_active_avg_members = avg.map(|a| (a * 10.0).round() / 10.0);

    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT name, value FROM metrics_counters WHERE day = DATE '1970-01-01' + $1",
    )
    .bind(d)
    .fetch_all(pool)
    .await?;
    s.counters = rows.into_iter().collect();
    Ok(s)
}

/// Computes and stores every closed day still covered by the raw rows that isn't stored
/// yet, plus yesterday again (late flushes from other nodes land within minutes).
pub async fn rollup(pool: &PgPool) -> anyhow::Result<usize> {
    let t = today();
    let done: Vec<i32> = sqlx::query_scalar(
        "SELECT (day - DATE '1970-01-01')::int FROM metrics_daily WHERE day >= DATE '1970-01-01' + $1",
    )
    .bind(t - RETENTION_DAYS)
    .fetch_all(pool)
    .await?;
    // Only days the metrics pipeline has seen: nothing to say before the first row.
    let first: Option<i32> =
        sqlx::query_scalar("SELECT (min(day) - DATE '1970-01-01')::int FROM metrics_activity")
            .fetch_one(pool)
            .await?;
    let Some(first) = first else { return Ok(0) };
    let mut n = 0;
    for d in first.max(t - RETENTION_DAYS)..t {
        if done.contains(&d) && d != t - 1 {
            continue;
        }
        let rolled = Rolled {
            people: day_stats(pool, d, false).await?,
            everyone: day_stats(pool, d, true).await?,
        };
        sqlx::query(
            "INSERT INTO metrics_daily (day, stats) VALUES (DATE '1970-01-01' + $1, $2)
             ON CONFLICT (day) DO UPDATE SET stats = excluded.stats, computed_at = now()",
        )
        .bind(d)
        .bind(serde_json::to_value(&rolled)?)
        .execute(pool)
        .await?;
        n += 1;
    }
    Ok(n)
}

/// A latency percentile from the `<name>_le_<ms>` buckets (upper bound of the bucket).
pub fn percentile(counters: &BTreeMap<String, i64>, name: &str, q: f64) -> Option<u64> {
    let mut buckets: Vec<(u64, i64)> = super::LATENCY_BUCKETS_MS
        .iter()
        .map(|b| {
            let key = if *b == u64::MAX {
                format!("{name}_le_inf")
            } else {
                format!("{name}_le_{b}")
            };
            (*b, counters.get(&key).copied().unwrap_or(0))
        })
        .collect();
    buckets.sort_by_key(|(b, _)| *b);
    let total: i64 = buckets.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return None;
    }
    let target = (total as f64 * q).ceil() as i64;
    let mut seen = 0;
    for (b, n) in buckets {
        seen += n;
        if seen >= target {
            return Some(b);
        }
    }
    None
}

#[derive(Serialize, Debug)]
pub struct Report {
    pub generated_for: String,
    pub population: &'static str,
    /// The two numbers that define success.
    pub headline: serde_json::Value,
    pub growth: serde_json::Value,
    pub retention: serde_json::Value,
    pub engagement: serde_json::Value,
    pub reliability: serde_json::Value,
    pub series: Vec<serde_json::Value>,
    /// Funnel and retention by acquisition source and by onboarding arm (ADR 0044).
    pub acquisition: serde_json::Value,
    /// Every running experiment, arm by arm, with guardrails (ADR 0044).
    pub experiments: serde_json::Value,
    pub everyone_headline: serde_json::Value,
    pub not_yet_measured: Vec<&'static str>,
}

fn headline(s: &DayStats) -> serde_json::Value {
    serde_json::json!({
        "mau": s.mau,
        "wau": s.wau,
        "dau": s.dau,
        "dau_over_mau": div(s.dau * 100, s.mau).map(|v| v / 100.0),
        "messages_per_active_user": {
            "day": div(s.messages_day, s.dau_senders),
            "week": div(s.messages_week, s.wau_senders),
            "month": div(s.messages_month, s.mau_senders),
        },
        "senders": { "dau": s.dau_senders, "wau": s.wau_senders, "mau": s.mau_senders },
        "messages": { "day": s.messages_day, "week": s.messages_week, "month": s.messages_month },
    })
}

/// Flushes nothing itself: callers flush first so the numbers include this node's counts.
pub async fn report(
    pool: &PgPool,
    everyone: bool,
    config: &roda_proto::experiments::RemoteConfig,
) -> anyhow::Result<Report> {
    rollup(pool).await?;
    let t = today();
    let live = Rolled {
        people: day_stats(pool, t, false).await?,
        everyone: day_stats(pool, t, true).await?,
    };
    let stored: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT stats FROM metrics_daily WHERE day > DATE '1970-01-01' + $1 - 30 AND day < DATE '1970-01-01' + $1
         ORDER BY day",
    )
    .bind(t)
    .fetch_all(pool)
    .await?;
    let mut days: Vec<Rolled> = stored
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    days.push(live.clone());
    let pick = |r: &Rolled| {
        if everyone {
            r.everyone.clone()
        } else {
            r.people.clone()
        }
    };
    let window: Vec<DayStats> = days.iter().map(pick).collect();
    let now = pick(&live);

    let mut a24 = Ratio::default();
    let mut a2 = Ratio::default();
    let (mut d1, mut d7, mut d30) = (Ratio::default(), Ratio::default(), Ratio::default());
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut signups = 0;
    for s in &window {
        a24.add(s.activation_24h);
        a2.add(s.activation_2_people_week1);
        d1.add(s.retention_d1);
        d7.add(s.retention_d7);
        d30.add(s.retention_d30);
        signups += s.signups;
        for (k, v) in &s.counters {
            *totals.entry(k.clone()).or_default() += v;
        }
    }
    let c = |k: &str| totals.get(k).copied().unwrap_or(0);
    let invites_accepted = c("invites_accepted");

    // Time to first message for invited accounts (live, accounts younger than the window).
    let invited_ttfm: Option<f64> = sqlx::query_scalar(
        "SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM first_message_at - signup_at))
         FROM metrics_accounts WHERE invited AND first_message_at IS NOT NULL AND kind = 0 AND (NOT test OR $1)",
    )
    .bind(everyone)
    .fetch_one(pool)
    .await?;

    // Weekly cohort curve: cohort i = signed up 7i..7i+6 days ago; week k = days 7k..7k+6
    // after signup. Shares of the cohort active in each week.
    let rows: Vec<(i32, i32, i64)> = sqlx::query_as(
        "SELECT (($1 - (cohort - DATE '1970-01-01')) / 7)::int AS i,
                ((day - cohort) / 7)::int AS k, count(DISTINCT pid)
         FROM metrics_activity
         WHERE cohort IS NOT NULL AND kind = 0 AND (NOT test OR $2) AND (sent > 0 OR syncs > 0)
         GROUP BY 1, 2",
    )
    .bind(t)
    .bind(everyone)
    .fetch_all(pool)
    .await?;
    let sizes: Vec<(i32, i64)> = sqlx::query_as(
        "SELECT (($1 - ((signup_at AT TIME ZONE 'UTC')::date - DATE '1970-01-01')) / 7)::int, count(*)
         FROM metrics_accounts WHERE kind = 0 AND (NOT test OR $2) GROUP BY 1",
    )
    .bind(t)
    .bind(everyone)
    .fetch_all(pool)
    .await?;
    let curve: Vec<serde_json::Value> = sizes
        .iter()
        .filter(|(i, _)| (0..5).contains(i))
        .map(|(i, size)| {
            let weeks: Vec<Option<f64>> = (0..=*i)
                .map(|k| {
                    let n = rows
                        .iter()
                        .find(|(ri, rk, _)| ri == i && *rk == k)
                        .map_or(0, |r| r.2);
                    pct(Ratio { num: n, den: *size })
                })
                .collect();
            serde_json::json!({ "cohort_weeks_ago": i, "size": size, "active_pct_by_week": weeks })
        })
        .collect();

    let spaces_created: BTreeMap<String, i64> = totals
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("spaces_created_")
                .map(|k| (k.to_string(), *v))
        })
        .collect();
    let send_ok = c("publish_ok");
    let send_failed = c("publish_rejected");

    let series = window
        .iter()
        .map(|s| {
            serde_json::json!({
                "day": s.day, "dau": s.dau, "wau": s.wau, "mau": s.mau,
                "messages": s.messages_day, "senders": s.dau_senders,
                "messages_per_sender": div(s.messages_day, s.dau_senders),
                "signups": s.signups,
            })
        })
        .collect();

    let (by_source, by_arm) = super::experiments::funnels(pool, config, everyone).await?;
    let experiments = super::experiments::experiments(pool, config, everyone, &totals).await?;
    let health_s = c("client_sessions");
    let health_c = c("client_crashes");

    Ok(Report {
        acquisition: serde_json::json!({
            "by_source": by_source,
            "by_onboarding_arm": by_arm,
        }),
        experiments,
        generated_for: now.day.clone(),
        population: if everyone {
            "everyone (QA/test accounts included)"
        } else {
            "people (no agents, no QA/test accounts)"
        },
        headline: headline(&now),
        everyone_headline: headline(&live.everyone),
        growth: serde_json::json!({
            "signups_30d": signups,
            "signups_today": now.signups,
            "activation_first_message_24h_pct": pct(a24),
            "activation_2_people_week1_pct": pct(a2),
            "invites_created_30d": c("invites_created"),
            "invites_accepted_30d": invites_accepted,
            "viral_k_30d": div(invites_accepted, signups),
            "invited_time_to_first_message_median_s": invited_ttfm.map(f64::round),
        }),
        retention: serde_json::json!({
            "d1_pct": pct(d1), "d7_pct": pct(d7), "d30_pct": pct(d30),
            "d1": d1, "d7": d7, "d30": d30,
            "weekly_cohorts": curve,
        }),
        engagement: serde_json::json!({
            "sessions_today": now.sessions,
            "sessions_per_dau": div(now.sessions, now.dau),
            "spaces_created_30d": spaces_created,
            "spaces_active_today": now.spaces_active,
            "spaces_active_avg_members": now.spaces_active_avg_members,
            "agent_messages_30d": now.agent_messages_month,
            "agent_messages_per_active_user_30d": div(now.agent_messages_month, now.mau),
            "agents_active_30d": now.agents_active_month,
            "approvals_approved_30d": c("approvals_approved"),
            "approvals_denied_30d": c("approvals_denied"),
            "media_blobs_30d": c("blobs_stored"),
        }),
        reliability: serde_json::json!({
            "send_latency_ms_p50": percentile(&totals, "send_ms", 0.5),
            "send_latency_ms_p95": percentile(&totals, "send_ms", 0.95),
            "time_to_first_sync_ms_p50": percentile(&totals, "first_sync_ms", 0.5),
            "time_to_first_sync_ms_p95": percentile(&totals, "first_sync_ms", 0.95),
            "sends_failed_30d": send_failed,
            "crash_free_sessions_pct_30d": (health_s > 0).then(|| ((1.0 - health_c as f64 / health_s as f64) * 10000.0).round() / 100.0),
            "crash_free_note": "opt-in app health reports only",
            "send_failure_pct_30d": pct(Ratio { num: send_failed, den: send_ok + send_failed }),
        }),
        series,
        not_yet_measured: vec!["client-side send latency (relay-side latency is reported instead)"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_from_buckets() {
        let mut c = BTreeMap::new();
        c.insert("send_ms_le_10".to_string(), 90);
        c.insert("send_ms_le_250".to_string(), 9);
        c.insert("send_ms_le_inf".to_string(), 1);
        assert_eq!(percentile(&c, "send_ms", 0.5), Some(10));
        assert_eq!(percentile(&c, "send_ms", 0.95), Some(250));
        assert_eq!(percentile(&c, "send_ms", 1.0), Some(u64::MAX));
        assert_eq!(percentile(&BTreeMap::new(), "send_ms", 0.5), None);
    }
}
