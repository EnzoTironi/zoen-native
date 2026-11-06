//! Explicit local inference: installed Codex owns login; no credentials are copied.
//! The production offline Gate floor is selected, not a claim of live Clef/finance.
use super::*;
use client_core::{Client, EntryKind, SecretVault};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct ExistingVault(PathBuf);
impl SecretVault for ExistingVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, _: String, _: Vec<u8>) -> bool {
        false
    }
    fn delete(&self, _: String) {}
}

fn owner(w: &World) -> TestResult<Arc<Client>> {
    let home = w.dir.join("bruno");
    let client = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )?;
    if !client.unlock(Arc::new(ExistingVault(home.join("vault"))))? {
        return Err("owner vault unavailable".into());
    }
    Ok(client)
}

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn counting_proxy(w: &World, binary: &str) -> TestResult<(PathBuf, PathBuf)> {
    let proxy = w.dir.join("counted-codex");
    let count = w.dir.join("codex-starts");
    std::fs::write(
        &proxy,
        format!(
            "#!/bin/sh\nset -eu\nprintf x >> {}\nexec {} \"$@\"\n",
            quote(count.to_str().ok_or("counter path")?),
            quote(binary)
        ),
    )?;
    std::fs::set_permissions(&proxy, std::fs::Permissions::from_mode(0o700))?;
    Ok((proxy, count))
}

fn start(w: &World, binary: &Path, source: &str, fault: bool, name: &str) -> TestResult<Host> {
    let mut command = w.cmd("agent", &["agent-host", "--source", source, "--for", "150"]);
    command
        .env_remove("ZOEN_AGENT_SCRIPT")
        .env("ZOEN_BRAIN", "codex-local")
        .env("ZOEN_CODEX_BIN", binary)
        .env("ZOEN_AGENT_GATE", "offline");
    if fault {
        command
            .env("ZOEN_AGENT_FAULT", "before-send")
            .env("ZOEN_AGENT_FAULT_MARKER", w.dir.join("model-captured"));
    } else {
        command
            .env_remove("ZOEN_AGENT_FAULT")
            .env_remove("ZOEN_AGENT_FAULT_MARKER");
    }
    let log = std::fs::File::create(w.dir.join(name))?;
    command.stdout(log.try_clone()?).stderr(log);
    Ok(Host(command.spawn()?))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit local Codex login/model usage and real PG/FDB relay required"]
async fn real_codex_thread_reply_recovers_capture_without_another_inference() -> TestResult {
    assert_eq!(std::env::var("ZOEN_TEST_REAL_CODEX").as_deref(), Ok("1"));
    let binary = std::env::var("ZOEN_CODEX_BIN")?;
    if !Path::new(&binary).is_absolute() {
        return Err("absolute installed Codex path required".into());
    }
    let w = World::new("real_codex_thread").await;
    w.init("bruno", "Bruno");
    let marker = types::new_id("real_ack")?;
    let definition: zoen_agentd::Definition = serde_json::from_value(serde_json::json!({
        "version":1,"kind":"user","persona":format!("Local synthetic QA. Answer each inbound request with exactly {marker}. Do not call tools or propose actions."),
        "rules":[],"state":{}
    }))?;
    let path = definition_file(&w, "definition.json", &definition);
    let agent = citizen(&w, "bruno", "agent", "Real model", "real_model", &path);
    w.zoen(
        "bruno",
        &[
            "agent-define",
            "@real_model",
            "--definition",
            path.to_str().ok_or("definition path")?,
        ],
    );
    w.zoen("bruno", &["dm", "@real_model"]);
    let client = owner(&w)?;
    client.start_sync(None)?;
    client.wait_until_settled(20_000).await?;
    let space = client
        .spaces()
        .into_iter()
        .find(|space| {
            space
                .counterpart
                .as_ref()
                .is_some_and(|person| person.id == agent)
        })
        .ok_or("direct chat")?
        .id;
    let root = client.send_message(space.clone(), "Synthetic topic".into())?;
    let source = client.send_reply(
        space.clone(),
        "Confirme com a resposta configurada.".into(),
        root.id.clone(),
        true,
    )?;
    client.wait_until_settled(20_000).await?;
    client.stop_sync();
    drop(client);

    let (proxy, counter) = counting_proxy(&w, &binary)?;
    let mut first = start(&w, &proxy, &source.id, true, "first-host.log")?;
    assert_eq!(
        exits(&mut first.0, 180),
        Some(86),
        "host must die at the existing post-capture/pre-send fault"
    );
    assert!(w.dir.join("model-captured").is_file());
    let calls = std::fs::read(&counter)?;
    assert!(!calls.is_empty(), "the actual Codex adapter must have run");
    drop(first);

    let mut recovered = start(&w, &proxy, &source.id, false, "recovered-host.log")?;
    read_until(&w, "bruno", "@real_model", &marker, &mut recovered.0);
    assert_eq!(exits(&mut recovered.0, 60), Some(0));
    assert_eq!(
        std::fs::read(&counter)?,
        calls,
        "recovery must not start Codex again"
    );
    drop(recovered);
    let client = owner(&w)?;
    let rows = client.timeline(space.clone())?;
    let answers: Vec<_> = rows
        .iter()
        .filter(|entry| {
            entry.author.id == agent
                && matches!(&entry.kind, EntryKind::Message { text, .. } if text == &marker)
        })
        .collect();
    assert_eq!(
        answers.len(),
        1,
        "the peer must receive one actual agent response"
    );
    let answer_id = answers[0].id.clone();
    assert_eq!(answers[0].in_thread.as_deref(), Some(root.id.as_str()));
    assert_eq!(
        answers[0].reply_to.as_ref().ok_or("exact source quote")?.id,
        source.id
    );
    drop(client);

    let mut again = start(&w, &proxy, &source.id, false, "third-host.log")?;
    assert_eq!(exits(&mut again.0, 60), Some(0));
    assert_eq!(std::fs::read(counter)?, calls);
    let reopened = owner(&w)?;
    let after = reopened.timeline(space)?;
    let ids: Vec<_> = after
        .iter()
        .filter(|entry| {
            entry.author.id == agent
                && matches!(&entry.kind, EntryKind::Message { text, .. } if text == &marker)
        })
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![answer_id.as_str()],
        "cold recovery must not publish a duplicate ID"
    );
    Ok(())
}
