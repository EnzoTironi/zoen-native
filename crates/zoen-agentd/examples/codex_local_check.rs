//! Explicit one-shot local operator check; never an ordinary test or hosted auth path.
use serde_json::json;
use std::{fs, io::Write, path::Path};
use zoen_agentd::{
    citizen::{BrainOut, CodexBrain},
    AgentKind, Brain, Definition, ModelTier, Turn, TurnContext,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const REPLY: &str = "Zoen local validation 0164";

#[cfg(unix)]
fn private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute()
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.mode() & 0o777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err("operator output must be an existing owned private directory".into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_directory(_: &Path) -> Result<()> {
    Err("operator directory validation is not qualified on this platform".into())
}

fn mark_once(directory: &Path) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut marker = options.open(directory.join("one-attempt.marker"))?;
    marker.write_all(b"One explicitly dispatched Brain turn; never retry automatically.\n")?;
    marker.sync_all()?;
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    if std::env::var("ZOEN_TEST_REAL_CODEX").as_deref() != Ok("1") {
        return Err("requires explicit ZOEN_TEST_REAL_CODEX=1 operator dispatch".into());
    }
    let directory = std::env::var_os("ZOEN_CODEX_LOCAL_QA_OUT")
        .map(std::path::PathBuf::from)
        .ok_or("requires an explicit private operator output directory")?;
    private_directory(&directory)?;
    mark_once(&directory)?;
    let brain = CodexBrain::from_env()?;
    let definition = Definition {
        version: 1,
        persona: format!(
            "Synthetic local check. Return outcome kind say with text exactly {REPLY:?}. \
             You have no tools. Do not request or perform external actions."
        ),
        rules: vec![],
        state: json!({}),
        kind: AgentKind::User,
        model_tier: ModelTier::Cheap,
    };
    let outcome = brain
        .think(
            TurnContext {
                agent: "synthetic-agent",
                device: "synthetic-device",
                owner: "synthetic-owner",
                definition: &definition,
            },
            &[],
            &Turn {
                space: "synthetic-space".into(),
                from: "synthetic-owner".into(),
                text: format!("Return exactly the say text {REPLY:?}."),
            },
        )
        .await;
    let accepted = matches!(&outcome, Ok(BrainOut::Say(text)) if text == REPLY);
    let receipt = json!({
        "status":if accepted {"PASS_PRODUCTION_CODEX_ADAPTER"}else{"FAIL_PRESERVED_OPERATOR_ATTEMPT"},
        "scope":"One synthetic production Brain turn, not Runtime/Gate/relay/native acceptance",
        "brainThinkCalls":1,
        "completedInference":accepted,
        "requestedModel":std::env::var("ZOEN_CODEX_MODEL").ok(),
        "exactSyntheticReply":accepted,
        "offeredTools":0,
        "credentialsReadOrCopiedByProbe":false,
        "providerInternalRetriesOrBillingMeasured":false,
        "runtimeOrDocumentEffect":false
    });
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(directory.join("outcome.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()?;
    if !accepted {
        return Err("production adapter did not complete the synthetic check; do not retry".into());
    }
    println!("PASS: one synthetic production Codex Brain turn; no tool or document action");
    Ok(())
}
