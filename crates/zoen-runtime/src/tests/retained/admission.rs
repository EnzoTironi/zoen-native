//! Paid calls here use an actual joined native image and only loopback HTTP.
mod cancellation;
mod deadlines;
mod progress;
mod sql_gate;

use super::*;

async fn original() -> (Fixture, ReplyRun) {
    let mut fixture = Fixture::new().await;
    fixture.grant("admitted-owner-trust").await;
    fixture.message(ORIGINAL).await;
    let mut runs = fixture.sync(false).await.unwrap().runs;
    assert_eq!(runs.len(), 1);
    (fixture, runs.remove(0))
}

fn binding(runtime: &RuntimeAuthority, step: &VerifiedStep) -> Binding {
    let dispatch = runtime.gateway.preflight(step.request()).unwrap();
    step.binding(&dispatch, &runtime.price).unwrap()
}

async fn cut(runtime: &RuntimeAuthority) {
    tokio::time::timeout(
        Duration::from_secs(3),
        runtime.native_cut_entered.notified(),
    )
    .await
    .unwrap();
}

async fn without_native(fixture: &Fixture) -> Arc<RuntimeAuthority> {
    fixture.world.reopen().await
}

async fn quote(fixture: &Fixture, run: &ReplyRun) -> i64 {
    fixture
        .world
        .runtime
        .execution
        .prepared_binding(&run.attempt, &fixture.world.runtime.custody)
        .await
        .unwrap()
        .hold_units
}

async fn restored(fixture: &Fixture) -> Arc<RuntimeAuthority> {
    let mut runtime = fixture.world.reopen().await;
    Arc::get_mut(&mut runtime).unwrap().native = Some(fixture.restore_custody());
    runtime
}

fn start(
    runtime: &Arc<RuntimeAuthority>,
    run: &ReplyRun,
) -> tokio::task::JoinHandle<Result<RetainedModel, RuntimeError>> {
    let runtime = runtime.clone();
    let run = run.run.clone();
    tokio::spawn(async move { runtime.run_model(&run).await })
}

async fn assert_zero_send(fixture: &Fixture) {
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.world.balance().await.spent_units, 0);
}

async fn assert_one_release(fixture: &Fixture) {
    assert_zero_send(fixture).await;
    assert_eq!(fixture.world.balance().await.held_units, 0);
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_dispositions WHERE kind='release'")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_postings WHERE kind='release'")
            .await,
        2
    );
}

pub(super) async fn run() {
    Box::pin(progress::run()).await;
    Box::pin(cancellation::run()).await;
    Box::pin(deadlines::run()).await;
    Box::pin(sql_gate::run()).await;
}
