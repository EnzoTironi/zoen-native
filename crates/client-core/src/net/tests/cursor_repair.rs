//! Original final-lost-wake regressions. No Signal code or fixture is copied.
use super::*;
use types::IdentityKind;

const SPACE: &str = "sp_final_unannounced";
const EVENT: &str = "last-unannounced-event";
const TEXT: &str = "O último evento também precisa chegar.";
const AT: i64 = 1_700_000_000_000;

fn policy() -> CursorRepairPolicy {
    // Short fixture policy, not a measurement of DEFAULT's 60s/30s budgets.
    CursorRepairPolicy {
        interval: Duration::from_millis(250),
        // The missing-Sync oracle expires before the watchdog fallback so its
        // negative control cannot be disguised as a transport-close failure.
        timeout: Duration::from_secs(5),
        initial_inactivity: Duration::from_secs(5),
    }
}

#[test]
fn ordered_partial_completions_cannot_complete_or_postpone_a_full_repair() {
    let start = tokio::time::Instant::now();
    let policy = policy();
    let mut repair = CursorRepair::new(policy, start);
    assert_eq!(
        repair.admission(),
        None,
        "initial catch-up has no polling timer"
    );
    assert!(repair.submitted(), "an overlapping initial Joined request");
    repair.completed(start);
    assert_eq!(repair.admission(), Some(start + policy.interval));
    assert_eq!(
        repair.abort_deadline(),
        Some(start + policy.interval + policy.timeout)
    );
    assert!(!repair.begin(start), "repair admission cannot run early");
    assert!(repair.begin(start + policy.interval));
    assert!(
        !repair.begin(start + policy.interval),
        "one full repair only"
    );
    assert!(repair.submitted(), "a gap queued AFTER the full repair");
    let deadline = start + policy.interval + policy.timeout;
    assert_eq!(repair.abort_deadline(), Some(deadline));
    repair.completed(start + policy.interval + Duration::from_millis(1));
    assert!(
        repair.in_flight(),
        "the older Joined completion is not the repair"
    );
    assert_eq!(repair.abort_deadline(), Some(deadline));
    let completed = start + policy.interval + Duration::from_millis(2);
    repair.completed(completed);
    assert!(!repair.in_flight());
    assert_eq!(repair.admission(), Some(completed + policy.interval));
    repair.completed(completed + Duration::from_millis(1));
    repair.completed(completed + Duration::from_millis(2));
    assert_eq!(
        repair.admission(),
        Some(completed + policy.interval),
        "later partial and unsolicited completions do not postpone reconciliation"
    );
    let fresh_socket = CursorRepair::new(policy, start);
    assert_eq!(fresh_socket.admission(), None);
    assert_eq!(
        fresh_socket.abort_deadline(),
        Some(start + policy.initial_inactivity)
    );
    assert!(
        !fresh_socket.in_flight(),
        "no repair admission survives reconnect"
    );
}

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_history(engine: &Engine, peer: &str) -> Vec<crate::TimelineEntry> {
    let rows = engine.store.events(SPACE).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row.client_id.as_str())
            .collect::<Vec<_>>(),
        ["repair-create", "repair-member", EVENT]
    );
    assert_eq!(
        rows.iter().map(|row| row.seq).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    let entries = engine.timeline(SPACE).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, EVENT);
    assert_eq!(entries[0].seq, 2);
    assert_eq!(entries[0].at_ms, AT + 2);
    assert_eq!(entries[0].author.id, peer);
    assert!(
        matches!(&entries[0].kind, crate::EntryKind::Message { text, card: None } if text == TEXT)
    );
    assert_eq!(engine.next_seq(SPACE), 3);
    assert!(engine.verify_log(SPACE).valid);
    entries
}

async fn ordinary(socket: &mut RelaySocket, frame: ClientFrame, marks: &mut Vec<u64>) {
    match frame {
        ClientFrame::Req { id, .. } => {
            reply(
                socket,
                ServerFrame::Res {
                    id,
                    result: Ok(Reply::Done),
                },
            )
            .await
        }
        ClientFrame::Ping => {
            reply(
                socket,
                ServerFrame::Pong {
                    server_time_ms: None,
                },
            )
            .await
        }
        ClientFrame::Ephemeral {
            space,
            kind: EphemeralKind::Delivered { seq },
        } => {
            assert_eq!(space, SPACE);
            marks.push(seq);
        }
        ClientFrame::Ephemeral { .. } => {}
        other => panic!("unexpected repair fixture traffic: {other:?}"),
    }
}

// The response is ordered behind prior inbound frames and adds no user content.
async fn barrier(
    command: &mpsc::UnboundedSender<Cmd>,
    socket: &mut RelaySocket,
    name: &str,
    marks: &mut Vec<u64>,
) {
    let (answer_to, answer) = oneshot::channel();
    command
        .send(Cmd::Req {
            op: Op::FetchLink { id: name.into() },
            reply: answer_to,
        })
        .unwrap();
    loop {
        let frame = request(socket).await;
        if let ClientFrame::Req {
            id,
            op: Op::FetchLink { id: link },
        } = &frame
        {
            if link == name {
                reply(
                    socket,
                    ServerFrame::Res {
                        id: *id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await;
                break;
            }
        }
        if let ClientFrame::Sync { cursors, all } = &frame {
            assert_ne!(
                name, "rejected-repair-content",
                "the active full repair cannot admit a second repair"
            );
            assert!(*all);
            assert_eq!(cursors.len(), 1);
            assert_eq!(cursors[0].space, SPACE);
            assert_eq!(
                cursors[0].next_seq,
                if name == "initial-complete" { 2 } else { 3 }
            );
            // A slow scheduler can legitimately reach a later interval while an
            // unrelated wire barrier is pending. Serve its current empty tail.
            reply(socket, ServerFrame::SyncDone).await;
            continue;
        }
        ordinary(socket, frame, marks).await;
    }
    assert!(matches!(answer.await.unwrap(), Ok(Reply::Link(None))));
}

#[tokio::test(flavor = "current_thread")]
async fn healthy_socket_recovers_final_unannounced_event_without_reconnect_or_followup() {
    socket_repair::healthy_socket_recovers_final_unannounced_event_without_reconnect_or_followup()
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn pending_repair_is_session_owned_and_pongs_cannot_mask_failure_or_stop() {
    session_failure::pending_repair_is_session_owned_and_pongs_cannot_mask_failure_or_stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_actual_dispatch_guard_cancels_a_stalled_await_and_drops_its_owner() {
    transport_deadlines::the_actual_dispatch_guard_cancels_a_stalled_await_and_drops_its_owner()
        .await;
}

#[test]
fn a_ready_work_exit_cannot_reenter_socket_close_after_the_repair_budget() {
    transport_deadlines::a_ready_work_exit_cannot_reenter_socket_close_after_the_repair_budget();
}

#[tokio::test(flavor = "current_thread")]
async fn a_close_started_before_the_deadline_cannot_outlive_the_same_repair_budget() {
    transport_deadlines::a_close_started_before_the_deadline_cannot_outlive_the_same_repair_budget(
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn expired_budget_and_stop_never_poll_another_transport_close() {
    transport_deadlines::expired_budget_and_stop_never_poll_another_transport_close().await;
}

#[tokio::test(flavor = "current_thread")]
async fn legitimate_full_completion_retargets_the_cleanup_budget_before_close() {
    transport_deadlines::legitimate_full_completion_retargets_the_cleanup_budget_before_close()
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn healthy_socket_discovers_new_authorized_space_when_joined_and_all_live_events_are_lost() {
    socket_repair::healthy_socket_discovers_new_authorized_space_when_joined_and_all_live_events_are_lost().await;
}

mod session_failure;
mod socket_repair;
mod transport_deadlines;
