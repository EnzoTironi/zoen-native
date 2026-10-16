//! Relay-owned visibility and ordered presence publication across nodes. Timestamps
//! never come from clients; the private policy is sent only to sibling devices.
use roda_proto::ServerFrame;
use sqlx::{pool::PoolConnection, Connection, PgPool, Postgres};

#[derive(Clone, Copy, Debug, Default)]
pub struct Preference {
    pub hide_last_seen: bool,
    pub last_seen_ms: Option<u64>,
    pub revision: Option<u64>,
}
impl Preference {
    pub fn frame(self, identity: String, online: bool, own: bool) -> ServerFrame {
        ServerFrame::Presence {
            identity,
            online: online && (own || !self.hide_last_seen),
            last_seen_ms: if online || self.hide_last_seen {
                None
            } else {
                self.last_seen_ms
            },
            hide_last_seen: own.then_some(self.hide_last_seen),
            revision: self.revision,
        }
    }
}
fn preference((hide_last_seen, seen, revision): (bool, Option<i64>, i64)) -> Preference {
    Preference {
        hide_last_seen,
        last_seen_ms: seen.and_then(|n| u64::try_from(n).ok()),
        revision: u64::try_from(revision).ok(),
    }
}
pub async fn get(pool: &PgPool, identity: &str) -> Result<Preference, sqlx::Error> {
    Ok(sqlx::query_as::<_, (bool, Option<i64>, i64)>(
        "SELECT hide_last_seen, last_seen_ms, revision FROM presence_preferences WHERE identity = $1",
    )
    .bind(identity)
    .fetch_optional(pool)
    .await?
    .map(preference)
    .unwrap_or_default())
}

/// Session advisory locks serialize *publication*, including the interval after a
/// durable SQL commit. Row/transaction locks alone release before that interval.
/// Sorted acquisition prevents snapshot deadlocks; hash collisions only serialize
/// unrelated accounts, never grant access. Cancellation closes this connection so
/// partially acquired session locks can never escape back into the pool.
struct Publication {
    connection: PoolConnection<Postgres>,
    ids: Vec<String>,
    locked: bool,
}
impl Drop for Publication {
    fn drop(&mut self) {
        if self.locked {
            self.connection.close_on_drop();
        }
    }
}
impl Publication {
    async fn acquire(pool: &PgPool, ids: &[String]) -> Result<Self, sqlx::Error> {
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        let mut this = Self {
            connection: pool.acquire().await?,
            ids,
            locked: true,
        };
        {
            let mut tx = this.connection.begin().await?;
            // Bound server-side waits too: cancelling a Rust future alone does not
            // interrupt PostgreSQL while it waits for an advisory lock.
            sqlx::query("SET LOCAL lock_timeout = '2s'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("SET LOCAL statement_timeout = '2s'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("SELECT pg_advisory_lock(key) FROM
                (SELECT DISTINCT hashtextextended(id, 782651) AS key FROM unnest($1::text[]) AS ids(id)) keys
                ORDER BY key")
                .bind(&this.ids).execute(&mut *tx).await?;
            tx.commit().await?;
        }
        Ok(this)
    }
    async fn release(mut self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "SELECT pg_advisory_unlock(key) FROM
             (SELECT DISTINCT hashtextextended(id, 782651) AS key FROM unnest($1::text[]) AS ids(id)) keys",
        )
        .bind(&self.ids)
        .execute(&mut *self.connection)
        .await?;
        self.locked = false;
        Ok(())
    }
    async fn policy(&mut self, identity: &str) -> Result<Preference, sqlx::Error> {
        Ok(sqlx::query_as::<_, (bool, Option<i64>, i64)>(
            "SELECT hide_last_seen, last_seen_ms, revision FROM presence_preferences WHERE identity = $1",
        )
        .bind(identity)
        .fetch_optional(&mut *self.connection)
        .await?
        .map(preference)
        .unwrap_or_default())
    }
}

pub async fn connected(
    state: &crate::AppState,
    identity: &str,
    peers: &[String],
    _announce: bool,
) -> Result<(), sqlx::Error> {
    let mut publication = Publication::acquire(&state.pool, &[identity.to_string()]).await?;
    let row = sqlx::query_as::<_, (bool, Option<i64>, i64)>("INSERT INTO presence_preferences(identity, revision) VALUES ($1, nextval('presence_revision'))
        ON CONFLICT (identity) DO UPDATE SET revision = nextval('presence_revision')
        RETURNING hide_last_seen, last_seen_ms, revision")
        .bind(identity).fetch_one(&mut *publication.connection).await?;
    let policy = preference(row);
    state.fanout.send(
        peers,
        &policy.frame(identity.to_string(), true, false),
        None,
    );
    // After Ready, synchronize the authoritative setting to all own devices.
    state.fanout.send(
        &[identity.to_string()],
        &policy.frame(identity.to_string(), true, true),
        None,
    );
    publication.release().await
}

pub async fn set_privacy(
    state: &crate::AppState,
    identity: &str,
    hide: bool,
    peers: &[String],
) -> Result<(), sqlx::Error> {
    let mut publication = Publication::acquire(&state.pool, &[identity.to_string()]).await?;
    // Autocommit completes before acknowledging; the advisory lock stays held.
    sqlx::query(
        "INSERT INTO presence_preferences (identity, hide_last_seen, revision) VALUES ($1, $2, nextval('presence_revision'))
        ON CONFLICT (identity) DO UPDATE SET hide_last_seen = EXCLUDED.hide_last_seen, revision = nextval('presence_revision')",
    )
    .bind(identity)
    .bind(hide)
    .execute(&mut *publication.connection)
    .await?;
    let policy = publication.policy(identity).await?;
    state.fanout.send(
        &[identity.to_string()],
        &policy.frame(identity.to_string(), true, true),
        None,
    );
    state.fanout.send(
        peers,
        &policy.frame(identity.to_string(), true, false),
        None,
    );
    publication.release().await
}

pub async fn offline(
    state: &crate::AppState,
    identity: &str,
    peers: &[String],
) -> Result<(), sqlx::Error> {
    let mut publication = Publication::acquire(&state.pool, &[identity.to_string()]).await?;
    if !state.fanout.is_online(identity).await {
        let row = sqlx::query_as::<_, (bool, Option<i64>, i64)>(
            "INSERT INTO presence_preferences (identity, last_seen_ms, revision)
             VALUES ($1, (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint, nextval('presence_revision'))
             ON CONFLICT (identity) DO UPDATE SET last_seen_ms = EXCLUDED.last_seen_ms, revision = nextval('presence_revision')
             RETURNING hide_last_seen, last_seen_ms, revision",
        )
        .bind(identity)
        .fetch_one(&mut *publication.connection)
        .await?;
        state.fanout.send(
            peers,
            &preference(row).frame(identity.to_string(), false, false),
            None,
        );
    }
    publication.release().await
}

/// Batched SQL and existing fanout lookup, under the same publication locks used
/// by live transitions. A delayed snapshot cannot overtake a newer notification.
pub async fn snapshot(
    state: &crate::AppState,
    identity: &str,
    peers: &[String],
) -> Result<(), sqlx::Error> {
    let mut publication = Publication::acquire(&state.pool, peers).await?;
    let rows = sqlx::query_as::<_, (String, bool, Option<i64>, i64)>(
        "SELECT wanted.id, COALESCE(p.hide_last_seen, false), p.last_seen_ms, COALESCE(p.revision, 0)
         FROM unnest($1::text[]) AS wanted(id)
         LEFT JOIN presence_preferences p ON p.identity = wanted.id",
    )
    .bind(peers)
    .fetch_all(&mut *publication.connection)
    .await?;
    let online: std::collections::HashSet<_> =
        state.fanout.online(peers).await.into_iter().collect();
    for (who, hide, seen, revision) in rows {
        if who != identity {
            let active = online.contains(&who);
            state.fanout.send(
                &[identity.to_string()],
                &preference((hide, seen, revision)).frame(who, active, false),
                None,
            );
        }
    }
    publication.release().await
}

pub async fn joined(state: &crate::AppState, identity: &str, audience: &[String]) {
    if !audience.iter().any(|id| id == identity) {
        return;
    }
    if snapshot(state, identity, audience).await.is_err() {
        return;
    }
    let Ok(mut publication) = Publication::acquire(&state.pool, &[identity.to_string()]).await
    else {
        return;
    };
    let Ok(policy) = publication.policy(identity).await else {
        return;
    };
    let online = state.fanout.is_online(identity).await;
    let peers: Vec<_> = audience
        .iter()
        .filter(|id| id.as_str() != identity)
        .cloned()
        .collect();
    state.fanout.send(
        &peers,
        &policy.frame(identity.to_string(), online, false),
        None,
    );
    let _ = publication.release().await;
}

/// Final websocket boundary: remote bus queues can reorder frames even when
/// publication is serialized. Re-read revision/policy and retain the shared row
/// fence through the actual write. A completed privacy update therefore cannot be
/// followed by a delayed public timestamp or stale sibling preference.
pub enum Delivery {
    Skip,
    Send(Option<sqlx::Transaction<'static, Postgres>>),
}
pub async fn delivery(
    state: &crate::AppState,
    recipient: &str,
    frame: &mut ServerFrame,
) -> Delivery {
    let ServerFrame::Presence {
        identity,
        online,
        last_seen_ms,
        hide_last_seen,
        revision,
    } = frame
    else {
        return Delivery::Send(None);
    };
    if let Some(expected) = *revision {
        if let Ok(mut tx) = state.pool.begin().await {
            let row = sqlx::query_as::<_, (bool, Option<i64>, i64)>(
                "SELECT hide_last_seen, last_seen_ms, revision FROM presence_preferences WHERE identity = $1 FOR SHARE",
            ).bind(&*identity).fetch_optional(&mut *tx).await;
            if let Ok(row) = row {
                let policy = row.map(preference).unwrap_or_default();
                if policy.revision.unwrap_or(0) != expected {
                    return Delivery::Skip;
                }
                if policy.hide_last_seen && identity != recipient {
                    *online = false;
                }
                *last_seen_ms = if *online || policy.hide_last_seen {
                    None
                } else {
                    policy.last_seen_ms
                };
                *hide_last_seen = if identity == recipient && hide_last_seen.is_some() {
                    Some(policy.hide_last_seen)
                } else {
                    None
                };
                return Delivery::Send(Some(tx));
            }
        }
    }
    // Legacy or outage signal: no private fields, and recompute online now rather
    // than letting an unversioned queued transition overwrite a newer state.
    *last_seen_ms = None;
    *hide_last_seen = None;
    *revision = None;
    *online = if identity == recipient {
        state.fanout.is_online(identity).await
    } else {
        false
    };
    Delivery::Send(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_policy_never_discloses_hidden_timestamp_or_peer_preference() {
        for online in [false, true] {
            for own in [false, true] {
                assert_eq!(
                    Preference {
                        hide_last_seen: true,
                        last_seen_ms: Some(42),
                        revision: None
                    }
                    .frame("ana".into(), online, own),
                    ServerFrame::Presence {
                        identity: "ana".into(),
                        online: online && own,
                        last_seen_ms: None,
                        revision: None,
                        hide_last_seen: own.then_some(true)
                    }
                );
            }
        }
        assert_eq!(
            Preference {
                hide_last_seen: false,
                last_seen_ms: Some(42),
                revision: None
            }
            .frame("ana".into(), false, false),
            ServerFrame::Presence {
                identity: "ana".into(),
                online: false,
                last_seen_ms: Some(42),
                revision: None,
                hide_last_seen: None
            }
        );
        assert_eq!(
            Preference {
                hide_last_seen: false,
                last_seen_ms: Some(42),
                revision: None
            }
            .frame("ana".into(), true, false),
            ServerFrame::Presence {
                identity: "ana".into(),
                online: true,
                last_seen_ms: None,
                revision: None,
                hide_last_seen: None
            }
        );
    }
}
