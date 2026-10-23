use super::*;
use event_log::{Author, Signer};
use types::EventBody;

/// Records each batch's client ids and answers each envelope with its own id. Each
/// batch waits for a permit, so a test can hold the worker while a backlog builds.
struct Recorder {
    batches: Mutex<Vec<(String, Vec<String>)>>,
    gate: Semaphore,
    entered: std::sync::atomic::AtomicUsize,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            batches: Mutex::default(),
            gate: Semaphore::new(0),
            entered: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Batcher for Recorder {
    type State = u32;

    async fn append_batch(&self, space: &str, batch: &[Pending], runs: &mut u32) -> Applied {
        self.entered
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.gate.acquire().await.expect("gate").forget();
        *runs += 1;
        let ids: Vec<String> = batch
            .iter()
            .map(|p| p.env.client_id().to_string())
            .collect();
        self.batches
            .lock()
            .unwrap()
            .push((space.to_string(), ids.clone()));
        Applied {
            results: ids.into_iter().map(|id| Err(Reject::no(id))).collect(),
            attempts: *runs,
        }
    }
}

fn fence(space: &str) -> Fence {
    Fence {
        partition: crate::ownership::partition_of(space),
        owner: "test".into(),
        token: 1,
    }
}

fn envelope(author: &Author, space: &str, i: u64) -> Envelope {
    let body = EventBody::MessagePosted {
        message: format!("m{i}"),
        text: format!("text {i}"),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    };
    Envelope::plain(&author.sign_event(space, &format!("c{i:04}"), 0, None, body))
}

fn reason(r: Result<Sequencing, Reject>) -> String {
    match r {
        Err(r) => r.reason,
        Ok(_) => panic!("the recorder rejects everything"),
    }
}

fn ids(n: u64) -> Vec<String> {
    (0..n).map(|i| format!("c{i:04}")).collect()
}

#[tokio::test]
async fn poisoned_queue_refuses_without_dispatch_or_retaining_admission_permits() {
    let rec = Arc::new(Recorder::default());
    let seq = Sequencer::new(rec.clone());
    let queues = seq.queues.clone();
    assert!(std::thread::spawn(move || {
        let _held = queues.lock().unwrap();
        panic!("controlled queue poison");
    })
    .join()
    .is_err());
    assert!(seq.active().is_err());
    let author = Author::root(Signer::generate());
    let (result, stats) = seq
        .append(envelope(&author, "poisoned", 0), true, fence("poisoned"))
        .await;
    let rejection = result.unwrap_err();
    assert!(!rejection.permanent);
    assert_eq!(rejection.reason, "log store unavailable");
    assert_eq!((stats.size, stats.attempts), (0, 0));
    assert_eq!(rec.entered.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(rec.batches.lock().unwrap().is_empty());
    assert_eq!(seq.count.available_permits(), MAX_PENDING);
    assert_eq!(seq.bytes.available_permits(), MAX_PENDING_BYTES);
}

#[tokio::test]
async fn backlog_commits_in_order_in_capped_batches() {
    let rec = Arc::new(Recorder::default());
    let seq = Arc::new(Sequencer::new(rec.clone()));
    let a = Author::root(Signer::generate());
    // Queue 1 + 120 envelopes while the first batch is held, in a known order.
    let first = tokio::spawn({
        let (seq, env) = (seq.clone(), envelope(&a, "s1", 0));
        async move { seq.append(env, true, fence("s1")).await }
    });
    while seq.active().unwrap() == 0 {
        tokio::task::yield_now().await;
    }
    tokio::task::yield_now().await;
    let mut rest = Vec::new();
    for i in 1..=120 {
        let (seq, env) = (seq.clone(), envelope(&a, "s1", i));
        rest.push(tokio::spawn(async move {
            seq.append(env, true, fence("s1")).await
        }));
        tokio::task::yield_now().await;
    }
    rec.gate.add_permits(100);

    let (r, stats) = first.await.unwrap();
    assert_eq!(reason(r), "c0000", "each caller gets its own result");
    assert_eq!(stats.size, 1);
    for (i, h) in rest.into_iter().enumerate() {
        let (r, _) = h.await.unwrap();
        assert_eq!(reason(r), format!("c{:04}", i + 1));
    }
    let batches = rec.batches.lock().unwrap().clone();
    let sizes: Vec<usize> = batches.iter().map(|(_, b)| b.len()).collect();
    assert_eq!(sizes, [1, MAX_BATCH, 120 - MAX_BATCH]);
    let order: Vec<String> = batches.into_iter().flat_map(|(_, b)| b).collect();
    assert_eq!(order, ids(121), "queue order is commit order");
}

#[tokio::test]
async fn spaces_have_their_own_workers_and_idle_ones_exit() {
    let rec = Arc::new(Recorder::default());
    rec.gate.add_permits(100);
    let seq = Sequencer {
        idle: Duration::from_millis(50),
        ..Sequencer::new(rec.clone())
    };
    let a = Author::root(Signer::generate());
    let (one, two) = tokio::join!(
        seq.append(envelope(&a, "s1", 0), true, fence("s1")),
        seq.append(envelope(&a, "s2", 1), true, fence("s2"))
    );
    assert_eq!(seq.active().unwrap(), 2);
    assert_eq!(
        (one.1.attempts, two.1.attempts),
        (1, 1),
        "state is per Space"
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(seq.active().unwrap(), 0, "idle workers leave");
    let (_, again) = seq.append(envelope(&a, "s1", 2), true, fence("s1")).await;
    assert_eq!(again.attempts, 1, "a new worker starts from fresh state");
    assert_eq!(seq.active().unwrap(), 1);
}
#[tokio::test]
async fn slow_space_rejects_excess_and_keeps_capacity_until_commit() {
    use futures_util::FutureExt;
    let rec = Arc::new(Recorder::default());
    let seq = Arc::new(Sequencer::new(rec.clone()));
    let a = Author::root(Signer::generate());
    let first = tokio::spawn({
        let (seq, env) = (seq.clone(), envelope(&a, "slow", 0));
        async move { seq.append(env, true, fence("slow")).await }
    });
    while rec.entered.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        tokio::task::yield_now().await;
    }
    // Drop the callers: pending jobs still own their capacity until committed.
    for i in 1..=SPACE_QUEUE {
        assert!(seq
            .append(envelope(&a, "slow", i as u64), true, fence("slow"))
            .now_or_never()
            .is_none());
    }
    let refused = seq
        .append(envelope(&a, "slow", 999), true, fence("slow"))
        .now_or_never()
        .unwrap()
        .0
        .unwrap_err();
    assert!(!refused.permanent);
    assert_eq!(seq.count.available_permits(), MAX_PENDING - SPACE_QUEUE - 1);
    rec.gate.add_permits(100);
    let _ = first.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while seq.count.available_permits() != MAX_PENDING {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(seq.bytes.available_permits(), MAX_PENDING_BYTES);
    assert_eq!(
        rec.batches
            .lock()
            .unwrap()
            .iter()
            .map(|(_, b)| b.len())
            .sum::<usize>(),
        SPACE_QUEUE + 1
    );
}

#[tokio::test]
async fn unsigned_invites_consume_bytes_while_batches_and_callers_are_stalled() {
    use futures_util::FutureExt;
    let rec = Arc::new(Recorder::default());
    let author = Author::root(Signer::generate());
    let mut env = envelope(&author, "one", 0);
    env.invite = Some("x".repeat(1024 * 1024 - 1024));
    assert!(env.verify().is_ok(), "invite bytes are unsigned");
    assert!(env.stored_len() < 1024);
    assert!(!env.valid_invite());
    let size = env.retained_len() + 512;
    let seq = Sequencer {
        bytes: Arc::new(Semaphore::new(size * 2)),
        ..Sequencer::new(rec.clone())
    };
    for space in ["one", "two"] {
        let mut queued = envelope(&author, space, 0);
        queued.invite = env.invite.clone();
        assert!(seq
            .append(queued, true, fence(space))
            .now_or_never()
            .is_none());
    }
    assert_eq!(
        seq.bytes.available_permits(),
        0,
        "abandoned callers cannot release job bytes"
    );
    let r = seq
        .append(env, true, fence("one"))
        .now_or_never()
        .unwrap()
        .0
        .unwrap_err();
    assert!(!r.permanent && r.reason.contains("capacity"));
    assert_eq!(seq.count.available_permits(), MAX_PENDING - 2);
    rec.gate.add_permits(2);
    tokio::time::timeout(Duration::from_secs(2), async {
        while seq.count.available_permits() != MAX_PENDING {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(seq.bytes.available_permits(), size * 2);
}

#[tokio::test]
async fn routing_across_spaces_still_obeys_process_count_and_byte_limits() {
    use futures_util::FutureExt;
    let rec = Arc::new(Recorder::default());
    let a = Author::root(Signer::generate());
    let seq = Sequencer {
        count: Arc::new(Semaphore::new(2)),
        ..Sequencer::new(rec.clone())
    };
    for space in ["one", "two"] {
        assert!(seq
            .append(envelope(&a, space, 0), true, fence(space))
            .now_or_never()
            .is_none());
    }
    let r = seq
        .append(envelope(&a, "three", 0), true, fence("three"))
        .now_or_never()
        .unwrap()
        .0
        .unwrap_err();
    assert!(!r.permanent);
    assert_eq!(seq.active().unwrap(), 2);
    let seq = Sequencer {
        bytes: Arc::new(Semaphore::new(1)),
        ..Sequencer::new(rec.clone())
    };
    let r = seq
        .append(envelope(&a, "bytes", 0), true, fence("bytes"))
        .now_or_never()
        .unwrap()
        .0
        .unwrap_err();
    assert!(!r.permanent);
    assert_eq!(seq.active().unwrap(), 0);
    assert_eq!(seq.count.available_permits(), MAX_PENDING);
    rec.gate.add_permits(100);
}
