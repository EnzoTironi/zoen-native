//! Daily export of closed days' **aggregate** numbers to PostHog, when configured:
//! `ZOEN_POSTHOG_HOST` (e.g. `https://us.i.posthog.com`) and `ZOEN_POSTHOG_KEY` (the
//! project's write-only key). One `zoen_daily_metrics` event per day, `distinct_id` =
//! `zoen-relay-<ZOEN_METRICS_ENV>`: never a person, never an id, only the numbers the admin
//! endpoint shows. PostHog draws the dashboard; nothing per user ever leaves the relay.

use sqlx::PgPool;

use super::report::{percentile, Rolled};

fn properties(r: &Rolled) -> serde_json::Value {
    let p = &r.people;
    let ratio = |x: super::report::Ratio| x.value().map(|v| (v * 1000.0).round() / 10.0);
    let per = |a: i64, b: i64| (b > 0).then(|| ((a as f64 / b as f64) * 100.0).round() / 100.0);
    let c = |k: &str| p.counters.get(k).copied().unwrap_or(0);
    serde_json::json!({
        "day": p.day,
        "mau": p.mau, "wau": p.wau, "dau": p.dau,
        "mau_senders": p.mau_senders, "wau_senders": p.wau_senders, "dau_senders": p.dau_senders,
        "dau_over_mau": per(p.dau, p.mau),
        "messages_day": p.messages_day,
        "messages_per_active_user_day": per(p.messages_day, p.dau_senders),
        "messages_per_active_user_week": per(p.messages_week, p.wau_senders),
        "messages_per_active_user_month": per(p.messages_month, p.mau_senders),
        "signups": p.signups,
        "activation_24h_pct": ratio(p.activation_24h),
        "activation_2_people_week1_pct": ratio(p.activation_2_people_week1),
        "retention_d1_pct": ratio(p.retention_d1),
        "retention_d7_pct": ratio(p.retention_d7),
        "retention_d30_pct": ratio(p.retention_d30),
        "invites_created": c("invites_created"),
        "invites_accepted": c("invites_accepted"),
        "viral_k_day": per(c("invites_accepted"), p.signups),
        "spaces_active": p.spaces_active,
        "sessions": p.sessions,
        "agent_messages": p.agent_messages_day,
        "approvals_approved": c("approvals_approved"),
        "approvals_denied": c("approvals_denied"),
        "media_blobs": c("blobs_stored"),
        "sends_failed": c("publish_rejected"),
        "send_latency_ms_p50": percentile(&p.counters, "send_ms", 0.5),
        "send_latency_ms_p95": percentile(&p.counters, "send_ms", 0.95),
        "mau_including_qa": r.everyone.mau,
        "$process_person_profile": false,
    })
}

/// Sends every closed, not yet exported day. Failures are retried on the next run.
pub async fn run(pool: &PgPool) {
    let (Ok(host), Ok(key)) = (
        std::env::var("ZOEN_POSTHOG_HOST"),
        std::env::var("ZOEN_POSTHOG_KEY"),
    ) else {
        return;
    };
    if let Err(e) = send(pool, host.trim_end_matches('/'), &key).await {
        tracing::warn!(error = %e, "metrics export to PostHog failed; retrying later");
    }
}

async fn send(pool: &PgPool, host: &str, key: &str) -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let env = std::env::var("ZOEN_METRICS_ENV").unwrap_or_else(|_| "dev".into());
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT to_char(day, 'YYYY-MM-DD'), stats FROM metrics_daily
         WHERE exported_at IS NULL AND day < DATE '1970-01-01' + $1 ORDER BY day LIMIT 60",
    )
    .bind(super::today())
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    for (day, stats) in rows {
        let Ok(rolled) = serde_json::from_value::<Rolled>(stats) else {
            continue;
        };
        let mut props = properties(&rolled);
        props["environment"] = env.clone().into();
        let body = serde_json::json!({
            "api_key": key,
            "event": "zoen_daily_metrics",
            "distinct_id": format!("zoen-relay-{env}"),
            // Noon UTC of the day, so every PostHog timezone puts it on that day.
            "timestamp": format!("{day}T12:00:00Z"),
            "uuid": uuid_for(&env, &day),
            "properties": props,
        });
        let r = client
            .post(format!("{host}/i/v0/e/"))
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&body)?)
            .send()
            .await?;
        anyhow::ensure!(r.status().is_success(), "PostHog answered {}", r.status());
        sqlx::query("UPDATE metrics_daily SET exported_at = now() WHERE day = $1::date")
            .bind(&day)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// A stable event id per environment and day, so a retried export doesn't count twice.
fn uuid_for(env: &str, day: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("zoen_daily_metrics\0{env}\0{day}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b[6] = (b[6] & 0x0f) | 0x50; // version 5-style
    b[8] = (b[8] & 0x3f) | 0x80;
    let x = hex::encode(b);
    format!(
        "{}-{}-{}-{}-{}",
        &x[..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..]
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_ids_are_stable_uuids() {
        let a = super::uuid_for("staging", "2026-10-08");
        assert_eq!(a, super::uuid_for("staging", "2026-10-08"));
        assert_ne!(a, super::uuid_for("staging", "2026-10-09"));
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "5");
    }
}
