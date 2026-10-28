//! Read-time statements are peer-clock evidence; database time controls retention only.
use protocol::read_time::{ReadTime, MAX_INTERVALS, MAX_MEMBERS};
use sqlx::PgPool;
type Key = (String, String, u64, u64);
pub(crate) async fn save(pool: &PgPool, rows: &[(Key, ReadTime)]) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut spaces = Vec::new();
    let mut peers = Vec::new();
    let mut after = Vec::new();
    let mut through = Vec::new();
    let mut times = Vec::new();
    for ((space, peer, a, b), time) in rows {
        spaces.push(space);
        peers.push(peer);
        after.push(a.to_string());
        through.push(b.to_string());
        times.push(time.reported_at_ms);
    }
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO space_read_times(space,identity,after_seq,through_seq,reported_at_ms)
        SELECT s,i,a::NUMERIC(20,0),b::NUMERIC(20,0),t FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::bigint[]) AS input(s,i,a,b,t)
        ON CONFLICT(space,identity,after_seq,through_seq) DO NOTHING")
        .bind(&spaces).bind(&peers).bind(&after).bind(&through).bind(&times).execute(&mut *tx).await?;
    sqlx::query(
        "DELETE FROM space_read_times WHERE observed_at < clock_timestamp()-INTERVAL '7 days'",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM space_read_times r USING (
        SELECT space,identity,after_seq,through_seq,ROW_NUMBER() OVER(PARTITION BY space,identity ORDER BY observed_at DESC,through_seq DESC) AS n
        FROM space_read_times WHERE (space,identity) IN(SELECT s,i FROM UNNEST($1::text[],$2::text[]) AS input(s,i))) old
        WHERE old.n>$3 AND r.space=old.space AND r.identity=old.identity AND r.after_seq=old.after_seq AND r.through_seq=old.through_seq")
        .bind(&spaces).bind(&peers).bind(i64::try_from(MAX_INTERVALS).map_err(|e|sqlx::Error::Protocol(e.to_string()))?)
        .execute(&mut *tx).await?;
    tx.commit().await
}
pub(crate) async fn read(
    pool: &PgPool,
    space: &str,
    peers: &[String],
) -> Result<Vec<(String, ReadTime)>, sqlx::Error> {
    if peers.len() > MAX_MEMBERS {
        return Err(sqlx::Error::Protocol("read time audience too large".into()));
    }
    let rows:Vec<(String,String,String,i64)>=sqlx::query_as("SELECT identity,after_seq::TEXT,through_seq::TEXT,reported_at_ms
        FROM (SELECT *,ROW_NUMBER() OVER(PARTITION BY identity ORDER BY observed_at DESC,through_seq DESC) AS n
        FROM space_read_times WHERE space=$1 AND identity=ANY($2) AND observed_at>=clock_timestamp()-INTERVAL '7 days') recent WHERE n<=$3")
        .bind(space).bind(peers).bind(i64::try_from(MAX_INTERVALS).map_err(|e|sqlx::Error::Protocol(e.to_string()))?).fetch_all(pool).await?;
    rows.into_iter()
        .map(|(who, a, b, at)| {
            let time = ReadTime {
                after_seq: a
                    .parse()
                    .map_err(|_| sqlx::Error::Protocol("invalid read prefix".into()))?,
                through_seq: b
                    .parse()
                    .map_err(|_| sqlx::Error::Protocol("invalid read prefix".into()))?,
                reported_at_ms: at,
            };
            if !time.valid() {
                return Err(sqlx::Error::Protocol("invalid read time evidence".into()));
            }
            Ok((who, time))
        })
        .collect()
}
