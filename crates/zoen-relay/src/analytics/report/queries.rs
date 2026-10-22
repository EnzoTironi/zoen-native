use super::*;

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
