//! Transport-local complete-read staging; no partial page reaches Engine/host.
use super::{
    remember_waiting, send, ClientFrame, Duration, Exit, HashMap, Op, Reply, Sink, SinkExt,
    StreamExt, Waiting,
};
use protocol::complete_reads::{Assembly, Kind, Request};

pub(super) fn start(op: Op, done: Waiting, owner: Option<&(String, String)>) -> (Op, Waiting) {
    let kind = match &op {
        Op::Devices => Some(Kind::Devices),
        Op::Agent(protocol::AgentRequest::State) => Some(Kind::AgentState),
        _ => None,
    };
    match (kind, owner) {
        (Some(kind), Some((identity, device))) => (
            Op::CompleteRead(Request::begin(kind)),
            Waiting::Paged {
                assembly: Box::new(Assembly::new(kind, identity.clone(), device.clone())),
                deadline: tokio::time::Instant::now() + Duration::from_secs(15),
                done: Box::new(done),
            },
        ),
        _ => (op, done),
    }
}
impl Waiting {
    pub(super) fn abandoned(&self) -> bool {
        match self {
            Self::External(tx) => tx.is_closed(),
            Self::Paged { done, .. } => done.abandoned(),
            _ => false,
        }
    }
    pub(super) fn fail(self, error: &str) {
        match self {
            Self::External(tx) => {
                let _ = tx.send(Err(error.into()));
            }
            Self::Paged { done, .. } => done.fail(error),
            _ => {}
        }
    }
}

pub(super) async fn page(
    id: u64,
    result: Result<Reply, String>,
    mut assembly: Box<Assembly>,
    done: Box<Waiting>,
    deadline: tokio::time::Instant,
    sink: &mut Sink,
    waiting: &mut HashMap<u64, Waiting>,
) -> Result<Option<(Waiting, Result<Reply, String>)>, Exit> {
    let result = match result {
        Ok(Reply::CompletePage(page)) => {
            if done.abandoned() || tokio::time::Instant::now() >= deadline {
                // Cancellation only releases this connection's derived snapshot.
                let request = Request {
                    kind: page.kind,
                    snapshot: page.snapshot,
                    ordinal: page.ordinal.saturating_add(1),
                    cancel: true,
                };
                send(
                    sink,
                    &ClientFrame::Req {
                        id,
                        op: Op::CompleteRead(request),
                    },
                )
                .await
                .map_err(Exit::Retry)?;
                return if done.abandoned() {
                    Ok(None)
                } else {
                    Ok(Some((*done, Err("complete_read_expired".into()))))
                };
            }
            match assembly.accept(page) {
                Ok(Some(request)) => {
                    send(
                        sink,
                        &ClientFrame::Req {
                            id,
                            op: Op::CompleteRead(request),
                        },
                    )
                    .await
                    .map_err(Exit::Retry)?;
                    waiting.insert(
                        id,
                        Waiting::Paged {
                            assembly,
                            done,
                            deadline,
                        },
                    );
                    return Ok(None);
                }
                Ok(None) => (*assembly).finish().map_err(str::to_owned),
                Err(error) => Err(error.into()),
            }
        }
        Ok(_) => Err(protocol::complete_reads::INVALID.into()),
        Err(error) => Err(error),
    };
    Ok(Some((*done, result)))
}

pub(super) type Deferred = std::collections::VecDeque<(Op, Waiting)>;
pub(super) fn active(waiting: &HashMap<u64, Waiting>) -> bool {
    waiting.values().any(|w| matches!(w, Waiting::Paged { .. }))
}
pub(super) async fn submit(
    op: Op,
    done: Waiting,
    sink: &mut Sink,
    waiting: &mut HashMap<u64, Waiting>,
    next_id: &mut u64,
    deferred: &mut Deferred,
) -> Result<(), String> {
    if matches!(&op,Op::CompleteRead(r) if r.snapshot.is_empty()) && active(waiting) {
        if deferred.len() >= 32 {
            done.fail("complete_read_busy");
        } else {
            deferred.push_back((op, done));
        }
        return Ok(());
    }
    if done.abandoned() {
        return Ok(());
    }
    let id = *next_id;
    *next_id = next_id.checked_add(1).ok_or("request id exhausted")?;
    if let Err(e) = send(sink, &ClientFrame::Req { id, op }).await {
        done.fail(&e);
        return Err(e);
    }
    remember_waiting(waiting, id, done);
    Ok(())
}
pub(super) fn deadline(waiting: &HashMap<u64, Waiting>) -> Option<tokio::time::Instant> {
    waiting
        .values()
        .filter_map(|w| match w {
            Waiting::Paged { deadline, .. } => Some(*deadline),
            _ => None,
        })
        .min()
}

pub(super) async fn drain(
    sink: &mut Sink,
    waiting: &mut HashMap<u64, Waiting>,
    next_id: &mut u64,
    deferred: &mut Deferred,
) -> Result<(), String> {
    if active(waiting) {
        return Ok(());
    }
    while let Some((op, done)) = deferred.pop_front() {
        if done.abandoned() {
            continue;
        }
        submit(op, done, sink, waiting, next_id, deferred).await?;
        break;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
