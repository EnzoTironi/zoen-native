use super::*;

impl Session {
    #[tracing::instrument(
        name = "sync",
        skip_all,
        fields(identity = %pseudo(&self.identity), all, spaces = Empty, events = Empty)
    )]
    pub(super) async fn sync(&mut self, cursors: Vec<protocol::Cursor>, all: bool) {
        let Ok(Ok(mine)) = tokio::time::timeout(
            Duration::from_secs(5),
            self.st.log.spaces_of(&self.identity),
        )
        .await
        else {
            self.send_sync(ServerFrame::error(
                ErrorCode::Unavailable,
                "database unavailable",
            ))
            .await;
            return;
        };
        // (space, from, through): a member reads to the head; someone removed reads up to
        // and including their removal, so their device learns of it, and nothing after.
        let mut plan = Vec::new();
        let current: std::collections::BTreeSet<_> = mine.iter().map(String::as_str).collect();
        for c in cursors {
            let member = current.contains(c.space.as_str());
            let removed_at = if member {
                None
            } else {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    self.st.log.removed_at(&c.space, &self.identity),
                )
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
            };
            if let Some(read) = relay_admission::sync::cursor_read(c, member, removed_at) {
                plan.push(read);
            }
        }
        relay_admission::sync::include_current(&mut plan, &mine, all);
        let span = tracing::Span::current();
        span.record("spaces", plan.len() as i64);
        let mut sent = 0usize;
        // One identity ceiling per eligible Space, never a per-connection copy
        // of all marks. Current membership is admitted again at begin and every
        // bounded keyset page; an admitted page may finish after a later removal.
        // PG receipt storage and the membership log are not one atomic store.
        let heads: Vec<String> = plan
            .iter()
            .filter(|read| read.through.is_none())
            .map(|read| read.space.clone())
            .collect();
        let Ok(Ok(current_receipt_spaces)) = tokio::time::timeout(
            Duration::from_secs(5),
            self.st.log.spaces_of(&self.identity),
        )
        .await
        else {
            self.send_sync(ServerFrame::error(
                ErrorCode::Unavailable,
                "membership unavailable",
            ))
            .await;
            return;
        };
        let current_receipt_spaces: std::collections::BTreeSet<_> =
            current_receipt_spaces.into_iter().collect();
        let heads: Vec<_> = heads
            .into_iter()
            .filter(|space| current_receipt_spaces.contains(space))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let Ok(Ok(mut ceilings)) = tokio::time::timeout(
            Duration::from_secs(5),
            self.st.receipts.ceilings(&self.st.pool, &heads),
        )
        .await
        else {
            self.send_sync(ServerFrame::error(
                ErrorCode::Unavailable,
                "database unavailable",
            ))
            .await;
            return;
        };
        for read in plan {
            let (space, mut next, mut through) = (read.space, read.from, read.through);
            #[cfg(debug_assertions)]
            if crate::log::sequencer::fork_histories::armed(&space) {
                let Ok(Ok(full)) = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.st.log.read_visible(
                        &space,
                        &self.identity,
                        0,
                        crate::log::sequencer::fork_histories::READ_LIMIT,
                    ),
                )
                .await
                else {
                    self.send_sync(ServerFrame::error(
                        ErrorCode::Unavailable,
                        "log store unavailable",
                    ))
                    .await;
                    return;
                };
                let Some(full) = full else { continue };
                through = through.or(full.through);
                let page = crate::log::sequencer::fork_histories::view(
                    &space,
                    &self.identity,
                    next,
                    full.events,
                );
                for ev in page {
                    if through.is_some_and(|last| ev.seq > last) {
                        break;
                    }
                    Metrics::inc(&self.st.metrics.sync_events_sent);
                    sent += 1;
                    if !self.send_sync(ServerFrame::Event { ev }).await {
                        return;
                    }
                }
                continue;
            }
            loop {
                let Ok(Ok(page)) = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.st
                        .log
                        .read_visible(&space, &self.identity, next, SYNC_PAGE),
                )
                .await
                else {
                    self.send_sync(ServerFrame::error(
                        ErrorCode::Unavailable,
                        "log store unavailable",
                    ))
                    .await;
                    return;
                };
                let Some(page) = page else { break };
                through = through.or(page.through);
                let n = page.events.len();
                for ev in page.events {
                    if through.is_some_and(|last| ev.seq > last) {
                        break;
                    }
                    next = ev.seq + 1;
                    Metrics::inc(&self.st.metrics.sync_events_sent);
                    sent += 1;
                    if !self.send_sync(ServerFrame::Event { ev }).await {
                        return;
                    }
                }
                if n < SYNC_PAGE || through.is_some_and(|last| next > last) {
                    break;
                }
            }
            if through.is_none() && !self.replay_receipts(&space, ceilings.remove(&space)).await {
                return;
            }
        }
        span.record("events", sent as i64);
        if !self.send_sync(ServerFrame::SyncDone).await {
            return;
        }
        self.st.analytics.synced(&self.identity);
        if !self.first_sync_done {
            self.first_sync_done = true;
            self.st
                .analytics
                .latency("first_sync_ms", self.started.elapsed());
        }
    }
}

impl Session {
    /// Deterministic identity order, each Delivered immediately followed by Read.
    /// Initial ceiling + monotonic, never-deleted identity rows guarantees every
    /// pre-cut row with >= its pre-cut mark, even across concurrent flush/raises.
    pub(super) async fn replay_receipts(&self, space: &str, ceiling: Option<String>) -> bool {
        let Some(ceiling) = ceiling else { return true };
        let mut after = String::new();
        loop {
            // The role read is the page admission boundary. A removed reader
            // starts no further page; a page admitted before removal may finish.
            match tokio::time::timeout(
                Duration::from_secs(5),
                self.st.log.role(space, &self.identity),
            )
            .await
            {
                Ok(Ok(Some(_))) => {}
                Ok(Ok(None)) => return true,
                _ => {
                    self.send_sync(ServerFrame::error(
                        ErrorCode::Unavailable,
                        "membership unavailable",
                    ))
                    .await;
                    return false;
                }
            }
            let Ok(Ok(page)) = tokio::time::timeout(
                Duration::from_secs(5),
                self.st
                    .receipts
                    .page(&self.st.pool, space, &after, &ceiling),
            )
            .await
            else {
                self.send_sync(ServerFrame::error(
                    ErrorCode::Unavailable,
                    "database unavailable",
                ))
                .await;
                return false;
            };
            for (from, delivered, read) in page.rows {
                for kind in [
                    protocol::EphemeralKind::Delivered { seq: delivered },
                    protocol::EphemeralKind::Read { seq: read },
                ] {
                    if !self
                        .send_sync(ServerFrame::Ephemeral {
                            space: space.into(),
                            from: from.clone(),
                            kind,
                        })
                        .await
                    {
                        return false;
                    }
                }
            }
            let Some(next) = page.next else { return true };
            after = next;
            tokio::task::yield_now().await;
        }
    }
}
