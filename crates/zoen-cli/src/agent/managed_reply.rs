//! Start one dedicated executor after the canonical agent has been enrolled.
use super::{image_ipc, Arc, Client, Duration};
use crate::{
    file_vault::FileVault,
    reply_enrollment_protocol::{Request, Response},
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdout},
};

pub(super) struct Worker {
    child: Child,
    drain: tokio::task::JoinHandle<Result<(), String>>,
    selected: bool,
    completed: Arc<AtomicBool>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.drain.abort();
        // Child also has kill_on_drop; its stdin pipe closes on abrupt parent
        // death, so the executor can retire safely even without this destructor.
    }
}

pub(super) async fn start(
    client: &Arc<Client>,
    home: &Path,
    source: Option<&zoen_runtime::ReplySource>,
) -> Result<Option<Worker>, String> {
    let Some((binary, config)) = image_ipc::reply_paths()? else {
        return Ok(None);
    };
    let account = client
        .account()
        .ok_or("managed reply account unavailable")?;
    let bytes = serde_json::to_vec(&Request {
        agent: account.identity_id,
        relay_url: account.relay_url,
    })
    .map_err(|_| "managed reply request unavailable")?;
    let response = image_ipc::reply_setup::<Response>("prepare-managed", &bytes).await?;
    let Response::Prepared { code } = response else {
        return Ok(None);
    };
    let vault = Arc::new(FileVault {
        dir: home.join("vault"),
    });
    client
        .link_runtime_device(code, vault)
        .await
        .map_err(|_| "managed reply enrollment unavailable")?;
    let initialized: serde_json::Value = image_ipc::reply_setup("initialize", &[]).await?;
    if initialized["status"] != "initialized" {
        return Err("managed reply initialization not confirmed".into());
    }
    let mut child = image_ipc::process(
        binary,
        config,
        if source.is_some() {
            "supervised-source"
        } else {
            "supervised-watch"
        },
    )
    .spawn()
    .map_err(|_| "managed reply spawn unavailable")?;
    if let Some(source) = source {
        let mut bytes = serde_json::to_vec(source).map_err(|_| "managed source invalid")?;
        if bytes.len() > 16384 {
            return Err("managed source too large".into());
        }
        bytes.push(b'\n');
        let input = child
            .stdin
            .as_mut()
            .ok_or("managed source pipe unavailable")?;
        input
            .write_all(&bytes)
            .await
            .map_err(|_| "managed source pipe unavailable")?;
        input
            .flush()
            .await
            .map_err(|_| "managed source pipe unavailable")?;
    }
    let mut output = BufReader::new(
        child
            .stdout
            .take()
            .ok_or("managed reply output unavailable")?,
    );
    let ready = tokio::time::timeout(Duration::from_secs(150), line(&mut output))
        .await
        .map_err(|_| "managed reply startup timed out")??;
    if ready.is_empty() {
        return Err("managed reply executor exited before readiness".into());
    }
    let frame: serde_json::Value =
        serde_json::from_slice(&ready).map_err(|_| "managed reply startup invalid")?;
    if frame["status"] != "worker_ready"
        || !frame["subject"].is_string()
        || !frame["notifications"].is_boolean()
    {
        return Err("managed reply readiness not confirmed".into());
    }
    let selected = source.is_some();
    let completed = Arc::new(AtomicBool::new(false));
    let observed = completed.clone();
    let drain = tokio::spawn(async move {
        let mut accepted = false;
        loop {
            let bytes = line(&mut output).await?;
            if bytes.is_empty() {
                if selected && accepted {
                    observed.store(true, Ordering::SeqCst);
                }
                return Ok(());
            }
            if selected {
                let frame: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| "managed source output invalid")?;
                if frame["status"] == "source_completed" {
                    accepted = true;
                }
            }
        }
    });
    Ok(Some(Worker {
        child,
        drain,
        selected,
        completed,
    }))
}

async fn line(output: &mut BufReader<ChildStdout>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    output
        .take(65537)
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|_| "managed reply output unavailable")?;
    if bytes.len() > 65536 {
        return Err("managed reply output too large".into());
    }
    Ok(bytes)
}

impl Worker {
    pub(super) fn check(&mut self) -> Result<bool, String> {
        let status = self
            .child
            .try_wait()
            .map_err(|_| "managed reply status unavailable")?;
        if let Some(status) = status {
            if status.success() && self.selected {
                if !self.drain.is_finished() {
                    return Ok(false);
                }
                if self.completed.load(Ordering::SeqCst) {
                    return Ok(true);
                }
            }
            return Err("managed reply executor stopped".into());
        }
        if self.drain.is_finished() && !(self.selected && self.completed.load(Ordering::SeqCst)) {
            return Err("managed reply output stopped".into());
        }
        Ok(false)
    }
    pub(super) async fn stop(&mut self) -> Result<(), String> {
        drop(self.child.stdin.take());
        let result = tokio::time::timeout(Duration::from_secs(150), self.child.wait())
            .await
            .map_err(|_| "managed reply shutdown timed out")?
            .map_err(|_| "managed reply shutdown unavailable")?;
        if !result.success() {
            return Err("managed reply shutdown failed".into());
        }
        Ok(())
    }
}
