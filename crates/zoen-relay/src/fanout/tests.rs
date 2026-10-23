use super::nats::{subject_for, subject_token};
use super::*;

/// Short deterministic resource benchmark: no NATS server or wall-time threshold.
#[tokio::test]
async fn presence_fanout_resource_benchmark() {
    use std::sync::atomic::AtomicUsize;
    for count in [32, 256, 4096] {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let requests = || {
            (0..count).map(|i| {
                let active = &active;
                let peak = &peak;
                async move {
                    let n = active.fetch_add(1, Relaxed) + 1;
                    peak.fetch_max(n, Relaxed);
                    tokio::task::yield_now().await;
                    active.fetch_sub(1, Relaxed);
                    (i % 2 == 0).then(|| i.to_string())
                }
            })
        };
        let before: Vec<_> = futures_util::future::join_all(requests())
            .await
            .into_iter()
            .flatten()
            .collect();
        let old_peak = peak.swap(0, Relaxed);
        let after = collect_presence(requests()).await;
        let new_peak = peak.load(Relaxed);
        assert_eq!(
            before, after,
            "membership and input order must survive batching"
        );
        assert_eq!(old_peak, count);
        assert_eq!(new_peak, count.min(PRESENCE_CONCURRENCY));
        assert_eq!(active.load(Relaxed), 0);
        println!("presence recipients={count}: outstanding before={old_peak}, after={new_peak}");
    }
}

/// 193 timed-out probes used to take seven waves (5.25s), exceeding login's 5s
/// timeout. Presence is best effort: keep successes and cancel the remaining work.
#[tokio::test]
async fn presence_lookup_deadline_keeps_partial_results() {
    let asks = (0..193).map(|i| async move {
        if i == 1 {
            return Some("online".to_string());
        }
        tokio::time::sleep(PRESENCE_TIMEOUT).await;
        None
    });
    let online = tokio::time::timeout(Duration::from_secs(3), collect_presence(asks))
        .await
        .expect("partial presence must return before the 5s login timeout");
    assert_eq!(online, vec!["online"]);
}

#[tokio::test]
async fn presence_deadline_cancels_stalled_probes_without_hiding_successes() {
    use std::sync::atomic::AtomicUsize;
    struct Active<'a>(&'a AtomicUsize);
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Relaxed);
        }
    }
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let asks = (0..193).map(|i| {
        let active = &active;
        let peak = &peak;
        async move {
            let count = active.fetch_add(1, Relaxed) + 1;
            peak.fetch_max(count, Relaxed);
            let _active = Active(active);
            if i == 1 {
                return Some("online".to_string());
            }
            std::future::pending::<Option<String>>().await
        }
    });
    let online = tokio::time::timeout(Duration::from_secs(3), collect_presence(asks))
        .await
        .expect("stalled probes must not block login");
    assert_eq!(online, vec!["online"]);
    assert_eq!(peak.load(Relaxed), PRESENCE_CONCURRENCY);
    assert_eq!(active.load(Relaxed), 0, "deadline must drop pending probes");
}

#[test]
fn subjects_are_pseudonymous_and_cell_scoped() {
    let a = subject_for("zoen.c0.to", "id_ana");
    assert!(a.starts_with("zoen.c0.to."));
    assert!(!a.contains("ana"));
    assert_eq!(a, subject_for("zoen.c0.to", "id_ana"));
    assert_ne!(a, subject_for("zoen.c1.to", "id_ana"));
    assert_eq!(subject_token("zoen t.1"), "zoen_t_1");
}
