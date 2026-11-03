use super::*;

impl Session {
    #[tracing::instrument(
        name = "ephemeral",
        skip_all,
        fields(identity = %pseudo(&self.identity), space = %pseudo(&space), delivered_here = Empty)
    )]
    pub(super) async fn ephemeral(&self, space: String, kind: protocol::EphemeralKind) -> bool {
        if self.st.limits.ephemeral_device.check(&self.device).is_err() {
            return true;
        }
        // One read: the member list answers both "may I?" and "who hears it?". Typing and
        // receipts are the most frequent frames, so a second transaction here doubled the
        // log store's read load for no extra information.
        let Ok(members) = self.st.log.members(&space).await else {
            return true;
        };
        if !members.iter().any(|(m, _)| *m == self.identity) {
            return true;
        }
        let kind = match kind {
            protocol::EphemeralKind::ReadTime(ref time) if !time.valid() => return true,
            protocol::EphemeralKind::ReadTime(time)
                if members.len() > protocol::read_time::MAX_MEMBERS =>
            {
                protocol::EphemeralKind::Read {
                    seq: time.through_seq,
                }
            }
            other => other,
        };
        let receipt = match kind {
            protocol::EphemeralKind::Read { seq } => Some((0, seq)),
            protocol::EphemeralKind::ReadTime(ref time) => Some((0, time.through_seq)),
            protocol::EphemeralKind::Delivered { seq } => Some((seq, 0)),
            _ => None,
        };
        if let Some((delivered, read)) = receipt {
            if !self
                .st
                .receipts
                .raise(&space, &self.identity, delivered, read)
            {
                Metrics::inc(&self.st.metrics.receipts_overloaded);
                self.send(ServerFrame::error(
                    ErrorCode::Unavailable,
                    "receipt backlog full; reconnect to replay",
                ))
                .await;
                return false;
            }
        }
        if let protocol::EphemeralKind::ReadTime(ref time) = kind {
            if !self.st.receipts.raise_time(&space, &self.identity, time) {
                self.send(ServerFrame::error(
                    ErrorCode::Unavailable,
                    "read time backlog full; reconnect to replay",
                ))
                .await;
                return false;
            }
        }
        // Receipts also reach my other devices (read on the phone clears the Mac), never
        // this session itself.
        let to: Vec<String> = match kind {
            protocol::EphemeralKind::Read { seq } | protocol::EphemeralKind::Delivered { seq }
                if members.len() > RECEIPT_BROADCAST_MAX =>
            {
                self.receipt_audience(&space, seq, &members).await
            }
            _ => members
                .into_iter()
                .map(|(m, _)| m)
                .filter(|m| receipt.is_some() || *m != self.identity)
                .collect(),
        };
        let n = self.st.fanout.send(
            &to,
            &ServerFrame::Ephemeral {
                space,
                from: self.identity.clone(),
                kind,
            },
            self.hub_id,
        );
        tracing::Span::current().record("delivered_here", n as i64);
        Metrics::inc(&self.st.metrics.ephemeral_forwarded);
        true
    }

    /// In a large Space a receipt goes to the authors of the messages it acknowledges and
    /// to my own devices, not to everyone: broadcasting every member's receipt for every
    /// message is O(members²) frames per message (a 32-person group at 250 msgs/s fell from
    /// 250 to 5 accepted msgs/s under it, SCALE.md §3). Authors are the ones whose ticks
    /// change. The watermark covers the last [`crate::recent::WINDOW`] messages up to `seq`
    /// live; an author further back learns it from the stored watermark on the next sync.
    pub(super) async fn receipt_audience(
        &self,
        space: &str,
        seq: u64,
        members: &[(String, types::Role)],
    ) -> Vec<String> {
        let authors = match self.st.fanout.recent.authors_through(space, seq) {
            Some(authors) => authors,
            None => {
                let window = crate::recent::WINDOW;
                let from = seq.saturating_sub(window as u64 - 1).max(1);
                match self.st.log.read(space, from, window).await {
                    Ok(events) => events
                        .iter()
                        .filter(|e| e.seq <= seq)
                        .map(|e| e.env.author().to_string())
                        .collect(),
                    Err(_) => Vec::new(),
                }
            }
        };
        // Intentional fresh native+portable contract correction: canonical old
        // authorship cannot grant receipt metadata after membership removal.
        relay_admission::receipts::recipients(
            &self.identity,
            &members
                .iter()
                .map(|(identity, _)| identity.clone())
                .collect::<Vec<_>>(),
            true,
            &authors,
        )
        .ok()
        .flatten()
        .unwrap_or_default()
    }
}
