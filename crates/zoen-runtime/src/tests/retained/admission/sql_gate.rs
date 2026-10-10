use super::*;

fn constraint(error: sqlx::Error) {
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}

pub(super) async fn run() {
    Box::pin(gate_without_attempt()).await;
    Box::pin(gate_after_reservation_and_claim()).await;
    Box::pin(admitted_sql_obligation_cannot_gate()).await;
}

async fn gate_without_attempt() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    let original = binding(&runtime, &step);
    step.release(&runtime.execution).await.unwrap();
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    constraint(sqlx::query("INSERT INTO runtime_attempts(attempt,owner,period_start,binding_digest,binding,hold_units) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&original.context.attempt_id).bind(&original.context.owner).bind(original.period_start)
        .bind(original.digest().unwrap()).bind(sqlx::types::Json(&original)).bind(original.hold_units)
        .execute(&runtime.finance.pool).await.unwrap_err());
    assert_eq!(
        runtime.finance.reserve(&original).await.unwrap_err(),
        RuntimeError::Denied
    );
    for mutation in [
        "UPDATE runtime_predispatch_closures SET binding_digest=repeat('0',64)",
        "DELETE FROM runtime_predispatch_closures",
        "TRUNCATE runtime_predispatch_closures",
    ] {
        constraint(
            sqlx::query(sqlx::AssertSqlSafe(mutation.to_owned()))
                .execute(&runtime.finance.pool)
                .await
                .unwrap_err(),
        );
    }
    constraint(sqlx::query("INSERT INTO runtime_predispatch_closures(attempt,owner,period_start,binding_digest) VALUES($1,$2,$3,$4)")
        .bind(new_id("no-original-period")).bind(&original.context.owner).bind(original.period_start + 1)
        .bind(original.digest().unwrap()).execute(&runtime.finance.pool).await.unwrap_err());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_periods")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_attempts")
            .await,
        0
    );
    assert_zero_send(&fixture).await;
    fixture.finish().await;
    println!("native SQL gate journey: raw insertion after pre-reservation closure and closure update/delete/truncate are refused, original period never fabricated PASS");
}

async fn gate_after_reservation_and_claim() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    let original = binding(&runtime, &step);
    runtime.finance.reserve(&original).await.unwrap();
    let claim = runtime.finance.claim(&original).await.unwrap().unwrap();
    drop(claim);
    constraint(sqlx::query("INSERT INTO runtime_predispatch_closures(attempt,owner,period_start,binding_digest) VALUES($1,$2,$3,$4)")
        .bind(&run.attempt).bind(&original.context.owner).bind(original.period_start)
        .bind(hash(b"wrong original binding")).execute(&runtime.finance.pool).await.unwrap_err());
    step.release(&runtime.execution).await.unwrap();
    assert_eq!(
        runtime.cancel_model(&run.run).await.unwrap(),
        ModelCancellation::Released
    );
    constraint(
        sqlx::query("INSERT INTO runtime_claims(attempt,nonce) VALUES($1,$2)")
            .bind(&run.attempt)
            .bind(hash(b"late claim after SQL gate"))
            .execute(&runtime.finance.pool)
            .await
            .unwrap_err(),
    );
    let nonce: String = sqlx::query_scalar("SELECT nonce FROM runtime_claims WHERE attempt=$1")
        .bind(&run.attempt)
        .fetch_one(&runtime.finance.pool)
        .await
        .unwrap();
    constraint(
        sqlx::query("INSERT INTO runtime_admission_witnesses(attempt,nonce) VALUES($1,$2)")
            .bind(&run.attempt)
            .bind(nonce)
            .execute(&runtime.finance.pool)
            .await
            .unwrap_err(),
    );
    constraint(sqlx::query("INSERT INTO runtime_dispositions(attempt,kind,amount_units,source_digest) VALUES($1,'settle',6,$2)")
        .bind(&run.attempt).bind(hash(b"late settle after SQL gate"))
        .execute(&runtime.finance.pool).await.unwrap_err());
    assert_one_release(&fixture).await;
    fixture.finish().await;
    println!("native SQL gate journey: claimed but proved-unsent attempt releases once; raw new claim, witness and settlement after closure are refused PASS");
}

async fn admitted_sql_obligation_cannot_gate() {
    let (fixture, run) = original().await;
    let runtime = fixture.world.runtime.clone();
    let step = runtime.verified_step(&run.run).await.unwrap();
    let original = binding(&runtime, &step);
    let (_, financial) = runtime.complete_verified(&step).await.unwrap();
    assert_eq!(financial, FinancialState::Settled { units: 6 });
    step.release(&runtime.execution).await.unwrap();
    constraint(sqlx::query("INSERT INTO runtime_predispatch_closures(attempt,owner,period_start,binding_digest) VALUES($1,$2,$3,$4)")
        .bind(&run.attempt).bind(&original.context.owner).bind(original.period_start)
        .bind(original.digest().unwrap()).execute(&runtime.finance.pool).await.unwrap_err());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_predispatch_closures")
            .await,
        0
    );
    assert_eq!(fixture.world.http.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.world.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 6
        }
    );
    fixture.finish().await;
    println!("native SQL gate journey: an actual known admitted and settled obligation cannot receive a predispatch closure PASS");
}
