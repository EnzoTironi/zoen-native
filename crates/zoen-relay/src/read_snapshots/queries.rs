use super::*;
use protocol::complete_reads::{Cursor, Device};
use sqlx::{Executor, Row};

pub(super) async fn membership(
    tx: &mut Transaction<'_, Postgres>,
    st: &Shared,
    identity: &str,
    version: i64,
) -> Result<(), String> {
    // Temporary, derived membership only; never writes the authoritative directory
    // or runtime cursor rows. Its lifetime is exactly this retained transaction.
    sqlx::query("CREATE TEMP TABLE complete_read_membership (space text COLLATE \"C\" PRIMARY KEY) ON COMMIT DROP")
        .execute(&mut **tx).await.map_err(|e|e.to_string())?;
    let mut last = String::new();
    let mut count = 0_usize;
    loop {
        let (spaces, complete) = st
            .log
            .membership_page(identity, version, &last, reads::PAGE_ROWS)
            .await
            .map_err(|e| e.to_string())?;
        if spaces.len() > reads::PAGE_ROWS || (!complete && spaces.is_empty()) {
            return Err(reads::INVALID.into());
        }
        for space in &spaces {
            if space.is_empty() || space.len() > reads::KEY_BYTES || space <= &last {
                return Err(reads::INVALID.into());
            }
            last = space.clone();
        }
        count = count
            .checked_add(spaces.len())
            .ok_or(reads::PROFILE_EXCEEDED)?;
        if count > reads::CURSOR_ROWS {
            return Err(reads::PROFILE_EXCEEDED.into());
        }
        sqlx::query("INSERT INTO complete_read_membership(space) SELECT unnest($1::text[])")
            .bind(&spaces)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        if complete {
            return Ok(());
        }
    }
}

pub(super) async fn header_in<'e, E: Executor<'e, Database = Postgres>>(
    executor: E,
    agent: &str,
) -> Result<(Header, String, String), String> {
    let row=sqlx::query("SELECT a.kind,a.definition_version,CASE WHEN octet_length(a.definition)<=65536 THEN a.definition ELSE NULL END AS definition,a.paused_at IS NOT NULL AS paused,a.revoked_at IS NOT NULL AS revoked,CASE WHEN octet_length(a.device)=64 THEN a.device ELSE NULL END AS device,CASE WHEN octet_length(a.owner)=64 THEN a.owner ELSE NULL END AS owner FROM agent_citizens a JOIN identities i ON i.id=a.agent WHERE a.agent=$1 AND i.kind='Agent' AND i.owner=a.owner")
        .bind(agent).fetch_optional(executor).await.map_err(|e|e.to_string())?.ok_or("not an agent runtime")?;
    let definition: Option<Vec<u8>> = row.try_get("definition").map_err(|e| e.to_string())?;
    let definition = definition.ok_or(reads::PROFILE_EXCEEDED)?;
    let paused: bool = row.try_get("paused").map_err(|e| e.to_string())?;
    let revoked: bool = row.try_get("revoked").map_err(|e| e.to_string())?;
    let version: i32 = row
        .try_get("definition_version")
        .map_err(|e| e.to_string())?;
    let kind: Option<String> = row.try_get("kind").map_err(|e| e.to_string())?;
    let header = Header {
        status: if revoked {
            "revoked"
        } else if paused {
            "paused"
        } else {
            "active"
        }
        .into(),
        kind: kind.unwrap_or_default(),
        definition_version: u32::try_from(version).map_err(|_| reads::INVALID)?,
        definition,
    };
    if !header.valid() {
        return Err(reads::INVALID.into());
    }
    Ok((
        header,
        row.try_get::<Option<String>, _>("owner")
            .map_err(|e| e.to_string())?
            .ok_or(reads::PROFILE_EXCEEDED)?,
        row.try_get::<Option<String>, _>("device")
            .map_err(|e| e.to_string())?
            .ok_or(reads::PROFILE_EXCEEDED)?,
    ))
}

pub(super) async fn devices(
    tx: &mut Transaction<'_, Postgres>,
    identity: &str,
    last: &str,
) -> Result<Vec<Device>, String> {
    // LIMIT bounds rows; CASE bounds each returned SQL field before allocation.
    // The raw indexed key still determines order, and malformed old rows refuse
    // the whole set instead of disappearing from its active/revoked projection.
    let rows:Vec<(Option<String>,bool)>=sqlx::query_as("SELECT CASE WHEN octet_length(device)=64 THEN device ELSE NULL END,revoked_at IS NOT NULL FROM devices WHERE identity=$1 AND device COLLATE \"C\">$2 COLLATE \"C\" ORDER BY device COLLATE \"C\" LIMIT $3")
        .bind(identity).bind(last).bind(reads::PAGE_ROWS as i64).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
    rows.into_iter()
        .map(|(device, revoked)| {
            Ok(Device {
                device: device.ok_or(reads::PROFILE_EXCEEDED)?,
                revoked,
            })
        })
        .collect()
}
pub(super) async fn cursors(
    tx: &mut Transaction<'_, Postgres>,
    agent: &str,
    last: &str,
) -> Result<Vec<Cursor>, String> {
    let rows:Vec<(Option<String>,i64)>=sqlx::query_as("SELECT CASE WHEN octet_length(c.space)<=128 THEN c.space ELSE NULL END,c.last_seq FROM agent_space_cursors c JOIN complete_read_membership m ON m.space=c.space WHERE c.agent=$1 AND c.space COLLATE \"C\">$2 COLLATE \"C\" ORDER BY c.space COLLATE \"C\" LIMIT $3")
        .bind(agent).bind(last).bind(reads::PAGE_ROWS as i64).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
    rows.into_iter()
        .map(|(space, seq)| {
            Ok(Cursor {
                space: space.ok_or(reads::PROFILE_EXCEEDED)?,
                seq: u64::try_from(seq).map_err(|_| reads::INVALID)?,
            })
        })
        .collect()
}
