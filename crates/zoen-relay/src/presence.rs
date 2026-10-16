//! Account-wide last-seen policy. Never accept timestamps from a client and never
//! put the private setting in a peer's presence frame.
use roda_proto::ServerFrame;
use sqlx::PgPool;

#[derive(Clone, Copy, Debug, Default)]
pub struct Preference {
    pub hide_last_seen: bool,
    pub last_seen_ms: Option<u64>,
}

impl Preference {
    pub fn frame(self, identity: String, online: bool, own: bool) -> ServerFrame {
        ServerFrame::Presence {
            identity,
            online,
            last_seen_ms: if online || self.hide_last_seen {
                None
            } else {
                self.last_seen_ms
            },
            hide_last_seen: own.then_some(self.hide_last_seen),
        }
    }
}

pub async fn get(pool: &PgPool, identity: &str) -> Result<Preference, sqlx::Error> {
    let row = sqlx::query_as::<_, (bool, Option<i64>)>(
        "SELECT hide_last_seen, last_seen_ms FROM presence_preferences WHERE identity = $1",
    )
    .bind(identity)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(hide_last_seen, last_seen_ms)| Preference {
            hide_last_seen,
            last_seen_ms: last_seen_ms.and_then(|n| u64::try_from(n).ok()),
        })
        .unwrap_or_default())
}

pub async fn set_privacy(pool: &PgPool, identity: &str, hide: bool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO presence_preferences (identity, hide_last_seen) VALUES ($1, $2)
        ON CONFLICT (identity) DO UPDATE SET hide_last_seen = EXCLUDED.hide_last_seen",
    )
    .bind(identity)
    .bind(hide)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn went_offline(pool: &PgPool, identity: &str) -> Result<Preference, sqlx::Error> {
    let (hide_last_seen, last_seen_ms) = sqlx::query_as::<_, (bool, i64)>(
        "INSERT INTO presence_preferences (identity, last_seen_ms)
         VALUES ($1, (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint)
         ON CONFLICT (identity) DO UPDATE SET last_seen_ms = EXCLUDED.last_seen_ms
         RETURNING hide_last_seen, last_seen_ms",
    )
    .bind(identity)
    .fetch_one(pool)
    .await?;
    Ok(Preference {
        hide_last_seen,
        last_seen_ms: u64::try_from(last_seen_ms).ok(),
    })
}

/// Batched directory read: no per-peer SQL work on a reconnect. Unknown timestamps
/// still produce an offline frame, clearing stale state retained by older clients.
pub async fn snapshot(
    pool: &PgPool,
    ids: &[String],
) -> Result<Vec<(String, Preference)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, bool, Option<i64>)>(
        "SELECT wanted.id, COALESCE(p.hide_last_seen, false), p.last_seen_ms
         FROM unnest($1::text[]) AS wanted(id)
         LEFT JOIN presence_preferences p ON p.identity = wanted.id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, hide_last_seen, last_seen_ms)| {
            (
                id,
                Preference {
                    hide_last_seen,
                    last_seen_ms: last_seen_ms.and_then(|n| u64::try_from(n).ok()),
                },
            )
        })
        .collect())
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
                        last_seen_ms: Some(42)
                    }
                    .frame("ana".into(), online, own),
                    ServerFrame::Presence {
                        identity: "ana".into(),
                        online,
                        last_seen_ms: None,
                        hide_last_seen: own.then_some(true)
                    }
                );
            }
        }
        assert_eq!(
            Preference {
                hide_last_seen: false,
                last_seen_ms: Some(42)
            }
            .frame("ana".into(), false, false),
            ServerFrame::Presence {
                identity: "ana".into(),
                online: false,
                last_seen_ms: Some(42),
                hide_last_seen: None
            }
        );
        assert_eq!(
            Preference {
                hide_last_seen: false,
                last_seen_ms: Some(42)
            }
            .frame("ana".into(), true, false),
            ServerFrame::Presence {
                identity: "ana".into(),
                online: true,
                last_seen_ms: None,
                hide_last_seen: None
            }
        );
    }
}
