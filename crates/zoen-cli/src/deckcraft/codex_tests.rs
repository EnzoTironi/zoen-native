use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit local Codex ChatGPT login and operator-signed real WASI worker"]
async fn operator_codex_brain_edits_through_runtime_gate_and_survives_cold_reopen() {
    assert_eq!(std::env::var("ZOEN_TEST_REAL_CODEX").as_deref(), Ok("1"));
    let worker = configured()
        .unwrap()
        .expect("explicit operator-signed worker required");
    let f = fixture();
    let steps = crate::agent::exercise_codex_deckcraft(
        &f.client,
        worker.clone(),
        &f.path.join("actual-codex-steps.json"),
        (&f.base.item_id, &f.source, &f.owner),
    )
    .await;
    let after = inspect(&f, &worker).await;
    assert_eq!(
        after["slides"][0]["notes"]["paragraphs"][0]["runs"][0]["text"],
        "Codex local model — gated edit."
    );
    let accepted = f
        .client
        .app_binary_read(f.base.item_id.clone(), None, None)
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert_ne!(accepted.sha256, f.base.sha256);
    let count = f
        .client
        .item(f.base.item_id.clone())
        .unwrap()
        .versions
        .len();
    assert_eq!(count, 2);
    let path = f.path.clone();
    let vault = f.vault.clone();
    let id = f.base.item_id.clone();
    drop(f);
    let cold = Client::open(
        path.join("agent.sqlite").to_str().unwrap().into(),
        "en".into(),
    )
    .unwrap();
    assert!(cold.unlock(vault).unwrap());
    assert_eq!(
        cold.app_binary_read(id.clone(), None, None).unwrap(),
        accepted
    );
    assert_eq!(cold.item(id).unwrap().versions.len(), 2);
    assert_eq!(
        serde_json::from_slice::<Vec<(String, Vec<zoen_agentd::ToolStep>)>>(
            &std::fs::read(path.join("actual-codex-steps.json")).unwrap()
        )
        .unwrap()[0]
            .1,
        steps
    );
    drop(cold);
    std::fs::remove_dir_all(path).unwrap();
    eprintln!("actual Codex ChatGPT model + Runtime/Gate + production signed worker + canonical receipt + cold SQLite: PASS; two tool steps, exactly version2, exact replay unchanged");
}
