use super::*;
use crate::read_snapshots::Snapshot;
use protocol::complete_reads::{self as reads, Assembly, Kind, Request};

impl Session {
    pub(super) async fn complete_read(&mut self, request: Request) -> Result<Reply, String> {
        if !self.complete_reads || !request.valid() {
            return Err("complete reads were not negotiated".into());
        }
        if self.read_snapshot.as_ref().is_some_and(Snapshot::expired) {
            self.read_snapshot = None;
        }
        if request.snapshot.is_empty() {
            if self.read_snapshot.is_some() {
                return Err("complete_read_busy".into());
            }
            let kind = Kind::try_from(request.kind).map_err(|_| reads::INVALID)?;
            self.read_snapshot =
                Some(Snapshot::begin(&self.st, &self.identity, &self.device, kind).await?);
        } else if !self
            .read_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(&request))
        {
            return Err("complete_read_stale".into());
        }
        let mut snapshot = self.read_snapshot.take().ok_or("complete_read_stale")?;
        if request.cancel {
            return Ok(Reply::Done);
        }
        let page = snapshot.next(&self.st).await?;
        // Heartbeat is explicitly an ordinary bounded SQL write. A lost Begin ACK
        // cannot prove it rolled back and must not authorize a mutation retry.
        if request.ordinal == 0 && request.kind == Kind::AgentState as i32 {
            heartbeat(&self.st, &self.identity, &self.device).await?;
        }
        if !page.complete {
            self.read_snapshot = Some(snapshot);
        }
        Ok(Reply::CompletePage(page))
    }
    /// Old wire variants remain complete-or-error, with finite query/reassembly
    /// and exact existing transport ceilings. Never disguise a prefix as a list.
    pub(super) async fn complete_legacy(&self, kind: Kind) -> Result<Reply, String> {
        let mut snapshot = Snapshot::begin(&self.st, &self.identity, &self.device, kind).await?;
        let mut staging = Assembly::new(kind, self.identity.clone(), self.device.clone());
        let mut bytes = 0_usize;
        loop {
            let page = snapshot.next(&self.st).await?;
            bytes = bytes
                .checked_add(page.wire_len())
                .ok_or(reads::PROFILE_EXCEEDED)?;
            // Leave room for old frame/protobuf headers before assembling/encoding.
            if bytes > MAX_FRAME - 1024 {
                return Err(reads::PROFILE_EXCEEDED.into());
            }
            if staging.accept(page).map_err(str::to_owned)?.is_none() {
                break;
            }
        }
        let reply = staging.finish().map_err(str::to_owned)?;
        if (ServerFrame::Res {
            id: u64::MAX,
            result: Ok(reply.clone()),
        })
        .encode()
        .len()
            > MAX_FRAME
        {
            return Err(reads::PROFILE_EXCEEDED.into());
        }
        if kind == Kind::AgentState {
            heartbeat(&self.st, &self.identity, &self.device).await?;
        }
        Ok(reply)
    }
}
async fn heartbeat(st: &Shared, identity: &str, device: &str) -> Result<(), String> {
    let result=sqlx::query("UPDATE agent_citizens SET last_seen_at=now() WHERE agent=$1 AND device=$2 AND revoked_at IS NULL")
        .bind(identity).bind(device).execute(&st.pool).await.map_err(|e|e.to_string())?;
    if result.rows_affected() != 1 {
        return Err("agent device changed".into());
    }
    Ok(())
}
