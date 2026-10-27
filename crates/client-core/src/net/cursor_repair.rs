use std::future::Future;

use tokio::sync::watch;

pub(super) use protocol::cursor_repair::CursorRepairPolicy;
pub(super) type CursorRepair = protocol::cursor_repair::CursorRepair<tokio::time::Instant>;

/// The guard surrounds the dispatcher, not just its read select: a blocked write
/// or request flush cannot prevent the repair deadline from being polled. Both
/// futures are local to the session and are dropped together; no task is spawned.
pub(super) async fn drive(
    work: impl Future<Output = super::Exit>,
    mut deadline: watch::Receiver<Option<tokio::time::Instant>>,
) -> (super::Exit, bool) {
    let expired = async {
        loop {
            let current = *deadline.borrow_and_update();
            match current {
                Some(until) => tokio::select! {
                    () = tokio::time::sleep_until(until) => {
                        // A completion may already have cleared/retargeted this
                        // timer while both select arms became ready.
                        if *deadline.borrow() == Some(until) { return; }
                    },
                    changed = deadline.changed() => {
                        if changed.is_err() {
                            tokio::time::sleep_until(until).await;
                            if *deadline.borrow() == Some(until) { return; }
                        }
                    }
                },
                None => {
                    if deadline.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                }
            }
        }
    };
    tokio::select! {
        exit = work => (exit, false),
        () = expired => (super::Exit::Retry("cursor reconciliation timed out".into()), true),
    }
}

/// Cleanup must not outlive the CURRENT repair budget after the dispatcher exits.
/// Stop/retirement or an incomplete initial catch-up abandons without polling
/// another transport write. Periodic cleanup retains its current deadline.
pub(super) async fn close_transport<T>(
    close: impl Future<Output = T>,
    deadline: Option<tokio::time::Instant>,
    abandon: bool,
) {
    if abandon || deadline.is_some_and(|until| tokio::time::Instant::now() >= until) {
        return;
    }
    if let Some(until) = deadline {
        let _ = tokio::time::timeout_at(until, close).await;
    } else {
        let _ = close.await;
    }
}
