//! Existing managed Codex stdio session transport and bounded RPC lifecycle.

use super::*;

impl Session {
    pub(super) async fn start(
        binary: &PathBuf,
        work: &PathBuf,
        deny: Option<&str>,
    ) -> Result<Self, String> {
        let mut c = command(binary);
        c.args(["app-server", "--listen", "stdio://", "--strict-config"])
            .current_dir(work);
        for flag in DISABLED {
            c.args(["-c", &format!("features.{flag}=false")]);
        }
        for setting in [
            "features.skip_host_skill_discovery=true",
            "mcp_servers={}",
            "skills.bundled.enabled=false",
            "web_search=\"disabled\"",
            "project_doc_max_bytes=0",
            "approval_policy=\"never\"",
            "sandbox_mode=\"read-only\"",
            "notify=[]",
            "instructions=\"\"",
            "developer_instructions=\"\"",
            "include_environment_context=false",
            "include_environment_context_time=false",
        ] {
            c.args(["-c", setting]);
        }
        if let Some(deny) = deny {
            c.args(["-c", deny]);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = c.spawn().map_err(|_| "Codex app-server unavailable")?;
        let input = child.stdin.take().ok_or("Codex stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().ok_or("Codex stdout unavailable")?);
        let mut s = Self {
            child,
            input,
            output,
            bytes: 0,
            next_id: 1,
            queued: std::collections::VecDeque::new(),
        };
        s.rpc("initialize",json!({"clientInfo":{"name":"zoen_local_brain","title":"Zoen local validation","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        s.write(&json!({"method":"initialized","params":{}}))
            .await?;
        Ok(s)
    }
    pub(super) async fn write(&mut self, value: &Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| "invalid Codex request")?;
        if bytes.len() > MAX_INPUT + 64 * 1024 {
            return Err("Codex request exceeds limit".into());
        }
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| "Codex request write failed".into())
    }
    pub(super) async fn next(&mut self) -> Result<Value, String> {
        if let Some(value) = self.queued.pop_front() {
            return Ok(value);
        }
        self.read_wire().await
    }
    pub(super) async fn read_wire(&mut self) -> Result<Value, String> {
        serde_json::from_str(&line(&mut self.output, &mut self.bytes).await?)
            .map_err(|_| "invalid Codex protocol JSON".into())
    }
    pub(super) async fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let value = self.read_wire().await?;
            if value.get("id").is_some() {
                if value.get("id").and_then(Value::as_u64) != Some(u64::from(id))
                    || value.get("method").is_some()
                {
                    return Err("unexpected Codex request/response".into());
                }
                return value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "Codex RPC refused".into());
            }
            // Initialization/thread-start notifications contain no permitted
            // external action. Never answer server requests or approvals.
            if method == "turn/start" {
                // Notifications can overtake the RPC response. Preserve them
                // within the same bounded wire budget for the completion owner.
                self.queued.push_back(value);
            } else if value.pointer("/params/item/type").is_some() {
                return Err("unexpected Codex item during admission".into());
            }
        }
    }
    pub(super) async fn close(&mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}
