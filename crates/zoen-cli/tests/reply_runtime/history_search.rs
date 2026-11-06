//! Actual native source read, cold query retry and current owner lifecycle.
use super::*;

pub(super) async fn run(
    w: &World,
    path: &Path,
    run: &str,
    space: &str,
    agent: &str,
    provider: &provider::Provider,
) -> TestResult {
    let command = json!({"command":"search_history","run":run,
        "arguments":{"query":"client closed","limit":1}});
    let first = call(path, "execute", Some(command.clone()))?;
    assert_eq!(first["status"], "history");
    let hits = first["page"]["hits"].as_array().ok_or("search hits")?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["space_id"], space);
    assert!(hits[0]["ref_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(hits[0]["persona"].is_object());
    assert_eq!(call(path, "execute", Some(command.clone()))?, first);
    for arguments in [
        json!({"query":"client closed","space":"other"}),
        json!({"query":"client closed","owner":"other"}),
        json!({"query":"client closed","approved":true}),
        json!({"query":"client closed","limit":21}),
    ] {
        assert!(call(
            path,
            "execute",
            Some(json!({"command":"search_history","run":run,"arguments":arguments}))
        )
        .is_err());
    }
    let paused = worker::lifecycle(w, agent, "Pause").await?;
    assert_eq!(paused["status"], "Paused");
    assert!(call(path, "execute", Some(command.clone())).is_err());
    let resumed = worker::lifecycle(w, agent, "Resume").await?;
    assert_eq!(resumed["status"], "Active");
    assert_eq!(call(path, "execute", Some(command))?, first);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    Ok(())
}
