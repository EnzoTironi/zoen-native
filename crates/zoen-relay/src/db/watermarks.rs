use super::*;

pub type ReceiptRow = ((String, String), (u64, u64));

fn receipt_decode(delivered: String, read: String) -> Result<(u64, u64), sqlx::Error> {
    let dto = relay_admission::receipts::WatermarkDto { delivered, read };
    relay_admission::receipts::Watermark::from_dto(&dto)
        .map(|mark| mark.pair())
        .map_err(|error| {
            sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid stored unsigned receipt: {error:?}"),
            )))
        })
}

pub async fn raise_receipts(pool: &sqlx::PgPool, batch: &[ReceiptRow]) -> Result<(), sqlx::Error> {
    let mut spaces = Vec::with_capacity(batch.len());
    let mut ids = Vec::with_capacity(batch.len());
    let mut delivered = Vec::with_capacity(batch.len());
    let mut read = Vec::with_capacity(batch.len());
    let mut sorted: Vec<_> = batch.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for ((space, who), (d, r)) in sorted {
        let dto = relay_admission::receipts::raise(None, *d, *r)
            .watermark
            .dto();
        spaces.push(space.as_str());
        ids.push(who.as_str());
        delivered.push(dto.delivered);
        read.push(dto.read);
    }
    sqlx::query(
        "INSERT INTO space_receipts(space,identity,delivered,read)
         SELECT space,identity,delivered::NUMERIC(20,0),read::NUMERIC(20,0)
         FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[])
         AS input(space,identity,delivered,read)
         ON CONFLICT(space,identity) DO UPDATE SET
           delivered=GREATEST(space_receipts.delivered,EXCLUDED.delivered),
           read=GREATEST(space_receipts.read,EXCLUDED.read)
         WHERE EXCLUDED.delivered > space_receipts.delivered OR EXCLUDED.read > space_receipts.read",
    ).bind(&spaces).bind(&ids).bind(&delivered).bind(&read).execute(pool).await.map(|_|())
}

/// One bounded initial ceiling per Space; identities are never deleted from this table.
pub async fn receipt_ceilings(
    pool: &PgPool,
    spaces: &[String],
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT space,MAX(identity) FROM space_receipts WHERE space=ANY($1) GROUP BY space",
    )
    .bind(spaces)
    .fetch_all(pool)
    .await
}

/// Sorted keyset page plus lookahead. No whole-Space receipt materialization.
pub async fn receipt_page(
    pool: &PgPool,
    space: &str,
    after: &str,
    ceiling: &str,
    limit: usize,
) -> Result<Vec<(String, u64, u64)>, sqlx::Error> {
    if limit == 0 || limit > relay_admission::receipts::PAGE_ROWS + 1 {
        return Err(sqlx::Error::Protocol("invalid receipt page size".into()));
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT identity,delivered::TEXT,read::TEXT FROM space_receipts
         WHERE space=$1 AND identity>$2 COLLATE \"C\" AND identity<=$3 COLLATE \"C\"
         ORDER BY identity COLLATE \"C\" LIMIT $4",
    )
    .bind(space)
    .bind(after)
    .bind(ceiling)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(who, d, r)| {
            let (d, r) = receipt_decode(d, r)?;
            Ok((who, d, r))
        })
        .collect()
}
