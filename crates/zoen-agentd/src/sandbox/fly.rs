//! Fly Machines as the microVM backend for staging (ADR 0028 P3). Fly has no nested KVM, so
//! staging can't run our own Firecracker; but every Fly Machine *is* a Firecracker microVM,
//! so a lease here is one Machine: its own kernel, its own disk, its own network identity.
//!
//! - **Private network**: the sandbox app lives on its own Fly network (`fly apps create
//!   --network`), so a Machine can't reach the relay, Postgres or FoundationDB over 6PN.
//! - **Lifecycle**: acquire = create + wait `started`; suspend = Fly's memory snapshot;
//!   resume = start; release = destroy (`force`). Nothing is pooled: a staging Machine costs
//!   only while it runs.
//! - **Exec and files** go through the Machines `exec` endpoint; files move as base64 on
//!   stdin/stdout, never through a shared mount.
//!
//! Not enforced here yet (P4, our own nodes): the `zoen-egress` allowlist (a Machine has
//! outbound internet), core-scheduling cookies and warm pools.

use super::*;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const API: &str = "https://api.machines.dev";
const FILE_CHUNK_BYTES: usize = 64 * 1024;
const MAX_API_RESPONSE: usize = 16 * 1024 * 1024;
/// Staging file-transfer ceiling; larger files are refused before transfer.
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct FlyConfig {
    pub api: String,
    pub app: String,
    pub token: String,
    /// The guest image (`ZOEN_FLY_SANDBOX_IMAGE`); Alpine by default.
    pub image: String,
    pub region: String,
}

impl std::fmt::Debug for FlyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the token.
        f.debug_struct("FlyConfig")
            .field("api", &self.api)
            .field("app", &self.app)
            .field("image", &self.image)
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl FlyConfig {
    /// `ZOEN_FLY_SANDBOX_APP` and `ZOEN_FLY_SANDBOX_TOKEN` (or `FLY_API_TOKEN`); `None` when either is missing.
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Some(FlyConfig {
            api: var("ZOEN_FLY_API").unwrap_or_else(|| API.into()),
            app: var("ZOEN_FLY_SANDBOX_APP")?,
            token: var("ZOEN_FLY_SANDBOX_TOKEN").or_else(|| var("FLY_API_TOKEN"))?,
            image: var("ZOEN_FLY_SANDBOX_IMAGE")
                .unwrap_or_else(|| "registry-1.docker.io/library/alpine:3.20".into()),
            region: var("ZOEN_FLY_REGION").unwrap_or_else(|| "gru".into()),
        })
    }
}

pub struct FlyMachinesProvider {
    cfg: FlyConfig,
    http: reqwest::Client,
    /// lease id → the Machine and the lease it backs.
    machines: Mutex<HashMap<String, (String, Lease)>>,
}

fn backend(e: impl std::fmt::Display) -> SandboxError {
    SandboxError::Backend(e.to_string())
}

/// Fly sizes memory in multiples of 256 MB.
fn fly_memory(mib: u32) -> u32 {
    mib.max(256).div_ceil(256) * 256
}

/// `argv` with `env` and `cwd` applied inside the guest.
fn wrap(req: &ExecRequest) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    if let Some(cwd) = &req.cwd {
        argv.extend([
            "/bin/sh".into(),
            "-c".into(),
            "cd \"$1\" && shift && exec \"$@\"".into(),
            "sh".into(),
            cwd.clone(),
        ]);
    }
    if !req.env.is_empty() {
        argv.push("/usr/bin/env".into());
        argv.extend(req.env.iter().map(|(k, v)| format!("{k}={v}")));
    }
    argv.extend(req.argv.iter().cloned());
    argv
}

fn cut(s: String) -> Bytes {
    let mut b = s.into_bytes();
    b.truncate(MAX_OUTPUT);
    Bytes::from(b)
}

impl FlyMachinesProvider {
    pub fn new(cfg: FlyConfig) -> Result<Self, SandboxError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(backend)?;
        Ok(FlyMachinesProvider {
            cfg,
            http,
            machines: Mutex::new(HashMap::new()),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1/apps/{}{path}", self.cfg.api, self.cfg.app)
    }

    async fn response(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, String), SandboxError> {
        let mut rb = self
            .http
            .request(method, self.url(path))
            .bearer_auth(&self.cfg.token);
        if let Some(b) = body {
            rb = rb
                .header("content-type", "application/json")
                .body(b.to_string());
        }
        let mut r = rb.send().await.map_err(backend)?;
        let status = r.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = r.chunk().await.map_err(backend)? {
            if chunk.len() > MAX_API_RESPONSE - bytes.len() {
                return Err(backend("fly response exceeds 16 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, String::from_utf8(bytes).map_err(backend)?))
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, SandboxError> {
        let (status, text) = self.response(method, path, body).await?;
        if !status.is_success() {
            // Fly's error bodies carry no secrets; keep them short.
            return Err(SandboxError::Backend(format!(
                "fly {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }
        Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::Null))
    }

    fn machine(&self, lease: &str) -> Result<String, SandboxError> {
        self.machines
            .lock()
            .unwrap()
            .get(lease)
            .map(|(m, _)| m.clone())
            .ok_or(SandboxError::NoLease)
    }

    async fn wait(&self, machine: &str, state: &str) -> Result<(), SandboxError> {
        self.call(
            reqwest::Method::GET,
            &format!("/machines/{machine}/wait?state={state}&timeout=60"),
            None,
        )
        .await
        .map(|_| ())
    }

    /// The Machine's state as Fly reports it (`started`, `suspended`, `destroyed`, …).
    pub async fn state_of(&self, machine: &str) -> Result<String, SandboxError> {
        let m = self
            .call(reqwest::Method::GET, &format!("/machines/{machine}"), None)
            .await?;
        Ok(m["state"].as_str().unwrap_or("").to_string())
    }

    /// The Machine behind a lease (for journeys that check it's gone).
    pub fn machine_of(&self, lease: &Lease) -> Option<String> {
        self.machines
            .lock()
            .unwrap()
            .get(&lease.id)
            .map(|(m, _)| m.clone())
    }

    async fn exec_raw(
        &self,
        machine: &str,
        argv: Vec<String>,
        stdin: Option<String>,
        timeout: Duration,
    ) -> Result<ExecOutput, SandboxError> {
        let started = Instant::now();
        let secs = timeout.as_secs().clamp(1, 3600);
        let mut body = serde_json::json!({ "command": argv, "timeout": secs });
        if let Some(s) = stdin {
            body["stdin"] = serde_json::Value::String(s);
        }
        let res = self
            .call(
                reqwest::Method::POST,
                &format!("/machines/{machine}/exec"),
                Some(body),
            )
            .await;
        let r = match res {
            Ok(r) => r,
            // Fly answers a command that outlives its timeout with an error; report it as one.
            Err(SandboxError::Backend(m)) if m.contains("deadline") || m.contains("timeout") => {
                return Ok(ExecOutput {
                    exit_code: None,
                    timed_out: true,
                    elapsed: started.elapsed(),
                    ..Default::default()
                })
            }
            Err(e) => return Err(e),
        };
        Ok(ExecOutput {
            exit_code: r["exit_code"].as_i64().map(|c| c as i32),
            stdout: cut(r["stdout"].as_str().unwrap_or("").to_string()),
            stderr: cut(r["stderr"].as_str().unwrap_or("").to_string()),
            timed_out: false,
            elapsed: started.elapsed(),
        })
    }

    async fn file_info(&self, machine: &str, path: &str) -> Result<(usize, String), SandboxError> {
        let out = self
            .exec_raw(
                machine,
                vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "n=$(stat -c %s -- \"$1\") && [ \"$n\" -le \"$2\" ] && printf '%s\\n' \"$n\" && sha256sum < \"$1\"".into(),
                    "sh".into(),
                    path.into(),
                    MAX_FILE_BYTES.to_string(),
                ],
                None,
                Duration::from_secs(60),
            )
            .await?;
        if out.exit_code != Some(0) {
            return Err(backend("file unavailable or exceeds 16 MiB"));
        }
        let text = std::str::from_utf8(&out.stdout).map_err(backend)?;
        let mut lines = text.lines();
        let size = lines
            .next()
            .ok_or_else(|| backend("missing file size"))?
            .parse::<usize>()
            .map_err(backend)?;
        if size > MAX_FILE_BYTES {
            return Err(SandboxError::Unsupported("transfer files over 16 MiB"));
        }
        let hash = lines
            .next()
            .and_then(|line| line.split_whitespace().next())
            .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| backend("missing file checksum"))?;
        Ok((size, hash.to_ascii_lowercase()))
    }
}

#[async_trait]
impl SandboxProvider for FlyMachinesProvider {
    async fn acquire(&self, spec: &SandboxSpec, owner: &str) -> Result<Lease, SandboxError> {
        let id = new_lease_id();
        // Fly's shared CPUs go up to 8; memory in 256 MB steps. No owner id leaves us: the
        // Machine's metadata names the lease only.
        let body = serde_json::json!({
            "region": self.cfg.region,
            "config": {
                "image": self.cfg.image,
                "init": { "exec": ["/bin/sleep", "inf"] },
                "guest": {
                    "cpu_kind": "shared",
                    "cpus": spec.vcpu.clamp(1, 8),
                    "memory_mb": fly_memory(spec.mem_mib),
                },
                "restart": { "policy": "no" },
                "auto_destroy": false,
                "metadata": { "zoen_lease": id, "zoen_tool": spec.tool },
            }
        });
        let m = self
            .call(reqwest::Method::POST, "/machines", Some(body))
            .await?;
        let machine = m["id"]
            .as_str()
            .ok_or_else(|| backend("fly: no machine id"))?
            .to_string();
        if let Err(e) = self.wait(&machine, "started").await {
            let _ = self
                .call(
                    reqwest::Method::DELETE,
                    &format!("/machines/{machine}?force=true"),
                    None,
                )
                .await;
            return Err(e);
        }
        let lease = Lease {
            id: id.clone(),
            owner: owner.to_string(),
            tool: spec.tool.clone(),
            tier: spec.tier,
        };
        self.machines
            .lock()
            .unwrap()
            .insert(id, (machine, lease.clone()));
        Ok(lease)
    }

    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError> {
        let machine = self.machine(&lease.id)?;
        let stdin = match &req.stdin {
            Some(b) => Some(
                String::from_utf8(b.to_vec())
                    .map_err(|_| SandboxError::Unsupported("binary stdin on Fly"))?,
            ),
            None => None,
        };
        let timeout = req.timeout;
        self.exec_raw(&machine, wrap(&req), stdin, timeout).await
    }

    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<(), SandboxError> {
        let machine = self.machine(&lease.id)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SandboxError::Unsupported("transfer files over 16 MiB"));
        }
        let out = self
            .exec_raw(
                &machine,
                vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "mkdir -p \"$(dirname -- \"$1\")\" && : > \"$1\"".into(),
                    "sh".into(),
                    path.into(),
                ],
                None,
                Duration::from_secs(60),
            )
            .await?;
        if out.exit_code != Some(0) {
            return Err(backend(format!(
                "put_file: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        for chunk in bytes.chunks(FILE_CHUNK_BYTES) {
            let out = self
                .exec_raw(
                    &machine,
                    vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "base64 -d >> \"$1\"".into(),
                        "sh".into(),
                        path.into(),
                    ],
                    Some(base64::engine::general_purpose::STANDARD.encode(chunk)),
                    Duration::from_secs(60),
                )
                .await?;
            if out.exit_code != Some(0) {
                return Err(backend("file upload failed"));
            }
        }
        let (size, hash) = self.file_info(&machine, path).await?;
        if size != bytes.len() || hash != hex::encode(Sha256::digest(&bytes)) {
            return Err(backend("file upload is incomplete or changed"));
        }
        Ok(())
    }

    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes, SandboxError> {
        let machine = self.machine(&lease.id)?;
        let (size, hash) = self.file_info(&machine, path).await?;
        let mut bytes = Vec::with_capacity(size);
        for offset in (0..size).step_by(FILE_CHUNK_BYTES) {
            let out = self
                .exec_raw(
                    &machine,
                    vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "dd if=\"$1\" bs=65536 skip=\"$2\" count=1 2>/dev/null | base64".into(),
                        "sh".into(),
                        path.into(),
                        (offset / FILE_CHUNK_BYTES).to_string(),
                    ],
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            if out.exit_code != Some(0) {
                return Err(backend("file download failed"));
            }
            let text: String = String::from_utf8_lossy(&out.stdout)
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let chunk = base64::engine::general_purpose::STANDARD
                .decode(text)
                .map_err(backend)?;
            if chunk.len() != FILE_CHUNK_BYTES.min(size - offset) {
                return Err(backend("file download is incomplete or changed"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if hex::encode(Sha256::digest(&bytes)) != hash {
            return Err(backend("file download checksum changed"));
        }
        Ok(Bytes::from(bytes))
    }

    async fn suspend(&self, lease: &Lease) -> Result<SnapshotRef, SandboxError> {
        let machine = self.machine(&lease.id)?;
        self.call(
            reqwest::Method::POST,
            &format!("/machines/{machine}/suspend"),
            None,
        )
        .await?;
        self.wait(&machine, "suspended").await?;
        Ok(SnapshotRef {
            id: machine,
            lease: lease.id.clone(),
        })
    }

    async fn resume(&self, snap: &SnapshotRef) -> Result<Lease, SandboxError> {
        let machine = self.machine(&snap.lease)?;
        self.call(
            reqwest::Method::POST,
            &format!("/machines/{machine}/start"),
            None,
        )
        .await?;
        self.wait(&machine, "started").await?;
        self.machines
            .lock()
            .unwrap()
            .get(&snap.lease)
            .map(|(_, l)| l.clone())
            .ok_or(SandboxError::NoLease)
    }

    async fn release(&self, lease: Lease) -> Result<(), SandboxError> {
        let machine = self
            .machines
            .lock()
            .unwrap()
            .get(&lease.id)
            .map(|(m, _)| m.clone());
        let Some(machine) = machine else {
            return Ok(());
        };
        let (status, text) = self
            .response(
                reqwest::Method::DELETE,
                &format!("/machines/{machine}?force=true"),
                None,
            )
            .await?;
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            return Err(backend(format!(
                "fly {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }
        self.machines.lock().unwrap().remove(&lease.id);
        Ok(())
    }
}
