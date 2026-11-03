//! Timestamp disclosure follows the same current-member small receipt audience.
use super::*;
impl Session {
    pub(super) async fn replay_read_times(&self, space: &str, rows: &[(String, u64, u64)]) -> bool {
        let Ok(members) = self.st.log.members(space).await else {
            return false;
        };
        if !members.iter().any(|(id, _)| id == &self.identity) {
            return true;
        }
        if members.len() > protocol::read_time::MAX_MEMBERS {
            return true;
        }
        let peers: Vec<_> = rows
            .iter()
            .map(|(id, _, _)| id)
            .filter(|id| members.iter().any(|(member, _)| member == *id))
            .cloned()
            .collect();
        let times = match self
            .st
            .receipts
            .read_times(&self.st.pool, space, &peers)
            .await
        {
            Ok(times) => times,
            Err(_) => {
                self.send_sync(ServerFrame::error(
                    ErrorCode::Unavailable,
                    "read time metadata unavailable",
                ))
                .await;
                return false;
            }
        };
        for (from, time) in times {
            if !self
                .send_sync(ServerFrame::Ephemeral {
                    space: space.into(),
                    from,
                    kind: protocol::EphemeralKind::ReadTime(time),
                })
                .await
            {
                return false;
            }
        }
        true
    }
}
