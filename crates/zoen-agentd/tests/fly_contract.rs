//! Offline Machines API regressions: cleanup retries and complete bounded file transfers.

use base64::Engine as _;
use bytes::Bytes;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use zoen_agentd::budget::DailyBudget;
use zoen_agentd::sandbox::fly::{FlyConfig, FlyMachinesProvider, MAX_FILE_BYTES};
use zoen_agentd::sandbox::{SandboxError, MAX_OUTPUT};
use zoen_agentd::session::{SessionError, ToolSession};
use zoen_agentd::{ExecRequest, SandboxProvider, SandboxSpec, Tier};

const NOW: i64 = 1_791_000_000_000;
const CHUNK_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct Guest {
    files: HashMap<String, Vec<u8>>,
    delete_failures: usize,
    delete_calls: usize,
    already_gone: bool,
    resume_failures: usize,
    resume_calls: usize,
    largest_stdin: usize,
    file_commands: usize,
    corrupt_download: bool,
    truncate_download: bool,
    truncate_upload: bool,
    oversized_file: bool,
    oversized_response: bool,
    state_response: Option<(u16, Value)>,
    #[cfg(target_os = "linux")]
    real_exec: bool,
}

impl Guest {
    fn answer(&mut self, method: &str, path: &str, body: Value) -> (u16, Value) {
        if method == "DELETE" {
            self.delete_calls += 1;
            if self.delete_failures > 0 {
                self.delete_failures -= 1;
                return (503, json!({"error": "temporarily unavailable"}));
            }
            return if self.already_gone {
                (404, json!({"error": "not found"}))
            } else {
                (200, json!({}))
            };
        }
        if path.ends_with("/start") {
            self.resume_calls += 1;
            if self.resume_failures > 0 {
                self.resume_failures -= 1;
                return (503, json!({"error": "temporarily unavailable"}));
            }
        }
        if method == "POST" && path.ends_with("/machines") {
            return (200, json!({"id": "machine-test"}));
        }
        if path.contains("/wait?") || path.ends_with("/suspend") || path.ends_with("/start") {
            return (200, json!({"ok": true}));
        }
        if method == "GET" {
            if let Some(response) = &self.state_response {
                return response.clone();
            }
            return if self.oversized_response {
                (
                    200,
                    json!({"state": "started", "padding": "x".repeat(16 * 1024 * 1024)}),
                )
            } else {
                (200, json!({"state": "started"}))
            };
        }
        let argv: Vec<&str> = body["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap())
            .collect();
        let stdin = body["stdin"].as_str().unwrap_or("");
        self.largest_stdin = self.largest_stdin.max(stdin.len());
        #[cfg(target_os = "linux")]
        if self.real_exec {
            use std::process::{Command, Stdio};
            let mut child = Command::new(argv[0])
                .args(&argv[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            return (
                200,
                json!({
                    "exit_code": out.status.code(),
                    "stdout": String::from_utf8_lossy(&out.stdout),
                    "stderr": String::from_utf8_lossy(&out.stderr),
                }),
            );
        }
        let script = argv.get(2).copied().unwrap_or("");
        let file = argv.get(4).copied().unwrap_or("");
        let stdout = if argv[0] == "/usr/bin/base64" {
            base64::engine::general_purpose::STANDARD.encode(&self.files[argv[1]])
        } else if script.contains("mkdir -p") {
            self.file_commands += 1;
            self.files.insert(
                file.into(),
                base64::engine::general_purpose::STANDARD
                    .decode(stdin)
                    .unwrap(),
            );
            String::new()
        } else if script.contains("base64 -d") {
            self.file_commands += 1;
            let mut bytes = base64::engine::general_purpose::STANDARD
                .decode(stdin)
                .unwrap();
            if self.truncate_upload {
                bytes.pop();
            }
            self.files.get_mut(file).unwrap().extend_from_slice(&bytes);
            String::new()
        } else if script.contains("stat -c") {
            self.file_commands += 1;
            let bytes = &self.files[file];
            let size = if self.oversized_file {
                MAX_FILE_BYTES + 1
            } else {
                bytes.len()
            };
            format!("{size}\n{}  {file}\n", hex::encode(Sha256::digest(bytes)))
        } else if script.contains("dd if=") {
            self.file_commands += 1;
            let offset = argv[5].parse::<usize>().unwrap() * CHUNK_BYTES;
            let file = &self.files[file];
            let mut chunk = file[offset..file.len().min(offset + CHUNK_BYTES)].to_vec();
            if self.corrupt_download && !chunk.is_empty() {
                chunk[0] ^= 1;
            }
            if self.truncate_download {
                chunk.pop();
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(chunk);
            encoded
                .as_bytes()
                .chunks(76)
                .map(|line| format!("{}\n", std::str::from_utf8(line).unwrap()))
                .collect()
        } else {
            "done\n".into()
        };
        (200, json!({"exit_code": 0, "stdout": stdout, "stderr": ""}))
    }
}

struct MockFly {
    api: String,
    guest: Arc<Mutex<Guest>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MockFly {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let guest = Arc::new(Mutex::new(Guest::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_guest, worker_stop) = (guest.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let (method, path, body) = read_request(&mut stream);
                        let (status, body) =
                            worker_guest.lock().unwrap().answer(&method, &path, body);
                        let body = body.to_string();
                        let headers = format!("HTTP/1.1 {status} reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = stream.write_all(headers.as_bytes());
                        let _ = stream.write_all(body.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("mock API: {error}"),
                }
            }
        });
        Self {
            api,
            guest,
            stop,
            worker: Some(worker),
        }
    }

    fn provider(&self) -> Arc<FlyMachinesProvider> {
        Arc::new(
            FlyMachinesProvider::new(FlyConfig {
                api: self.api.clone(),
                app: "sandbox-test".into(),
                token: "test-only".into(),
                image: "test-image".into(),
                region: "test".into(),
            })
            .unwrap(),
        )
    }
}

impl Drop for MockFly {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn read_request(stream: &mut TcpStream) -> (String, String, Value) {
    let mut bytes = Vec::new();
    let mut buf = [0; 8192];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert_ne!(n, 0, "incomplete HTTP headers");
        bytes.extend_from_slice(&buf[..n]);
        if let Some(end) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
    let mut request = headers.lines().next().unwrap().split_whitespace();
    let (method, path) = (
        request.next().unwrap().to_string(),
        request.next().unwrap().to_string(),
    );
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while bytes.len() - end < length {
        let n = stream.read(&mut buf).unwrap();
        assert_ne!(n, 0, "incomplete HTTP body");
        bytes.extend_from_slice(&buf[..n]);
    }
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[end..end + length]).unwrap()
    };
    (method, path, body)
}

fn spec() -> SandboxSpec {
    SandboxSpec {
        tier: Tier::MicroVm,
        tool: "test-tool".into(),
        template: "base".into(),
        vcpu: 2,
        mem_mib: 256,
        disk_mib: 1024,
        max_secs: 120,
        egress: vec![],
        secrets: vec![],
        agent: None,
    }
}

fn session(provider: Arc<FlyMachinesProvider>) -> ToolSession {
    ToolSession::new(
        provider,
        spec(),
        "ana".into(),
        Duration::ZERO,
        Arc::new(Mutex::new(DailyBudget::new(3600, 0, NOW))),
    )
}

#[tokio::test]
async fn destruction_proof_requires_a_confirmed_state_or_not_found() {
    let api = MockFly::new();
    let fly = api.provider();
    for (status, body, expected) in [
        (404, json!({"error": "not found"}), Some("destroyed")),
        (200, json!({"state": "destroyed"}), Some("destroyed")),
        (200, json!({"state": "destroying"}), Some("destroying")),
        (200, json!({"state": "started"}), Some("started")),
        (200, json!({}), None),
        (200, json!({"state": ""}), None),
        (401, json!({"error": "unauthorized"}), None),
        (403, json!({"error": "forbidden"}), None),
        (408, json!({"error": "request timeout"}), None),
        (500, json!({"error": "internal error"}), None),
        (503, json!({"error": "unavailable"}), None),
    ] {
        api.guest.lock().unwrap().state_response = Some((status, body));
        let proof = fly.state_of("machine-test").await;
        match expected {
            Some(state) => assert_eq!(proof.unwrap(), state),
            None => assert!(proof.is_err(), "HTTP {status}: {proof:?}"),
        }
    }
    drop(api);
    assert!(
        fly.state_of("machine-test").await.is_err(),
        "a transport failure cannot prove destruction",
    );
}

#[tokio::test]
async fn failed_delete_keeps_the_machine_until_a_successful_retry() {
    let api = MockFly::new();
    api.guest.lock().unwrap().delete_failures = 1;
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    assert!(fly.release(lease.clone()).await.is_err());
    assert_eq!(fly.machine_of(&lease).as_deref(), Some("machine-test"));
    fly.release(lease.clone()).await.unwrap();
    assert_eq!(fly.machine_of(&lease), None);
    fly.release(lease).await.unwrap();
    assert_eq!(api.guest.lock().unwrap().delete_calls, 2);
}

#[tokio::test]
async fn an_already_destroyed_machine_is_successful_idempotent_cleanup() {
    let api = MockFly::new();
    api.guest.lock().unwrap().already_gone = true;
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    fly.release(lease.clone()).await.unwrap();
    assert_eq!(fly.machine_of(&lease), None);
    fly.release(lease).await.unwrap();
    assert_eq!(api.guest.lock().unwrap().delete_calls, 1);
}

#[tokio::test]
async fn finishing_retries_cleanup_without_new_exec_or_duplicate_usage() {
    let api = MockFly::new();
    api.guest.lock().unwrap().delete_failures = 1;
    let session = session(api.provider());
    session
        .exec(ExecRequest::sh("echo done"), NOW)
        .await
        .unwrap();
    assert!(session.finish(NOW).await.is_err());
    assert_eq!(session.state().await, "finishing");
    assert!(session.usage().is_empty());
    assert!(matches!(
        session.exec(ExecRequest::sh("echo again"), NOW).await,
        Err(SessionError::Finished)
    ));
    session.finish(NOW).await.unwrap();
    session.finish(NOW).await.unwrap();
    assert_eq!(session.state().await, "done");
    assert_eq!(session.usage().len(), 1);
    assert_eq!(api.guest.lock().unwrap().delete_calls, 2);
}

#[tokio::test]
async fn suspended_cleanup_can_retry_resume_then_retry_delete() {
    let api = MockFly::new();
    {
        let mut guest = api.guest.lock().unwrap();
        guest.resume_failures = 1;
        guest.delete_failures = 1;
    }
    let session = session(api.provider());
    session
        .exec(ExecRequest::sh("echo done"), NOW)
        .await
        .unwrap();
    assert!(session.tick(NOW).await.unwrap());
    assert_eq!(session.state().await, "suspended");
    assert!(session.finish(NOW).await.is_err());
    assert_eq!(session.state().await, "suspended");
    assert!(session.finish(NOW).await.is_err());
    assert_eq!(session.state().await, "finishing");
    session.finish(NOW).await.unwrap();
    assert_eq!(session.state().await, "done");
    assert_eq!(session.usage().len(), 1);
    let guest = api.guest.lock().unwrap();
    assert_eq!((guest.resume_calls, guest.delete_calls), (2, 2));
}

#[tokio::test]
async fn binary_files_over_the_exec_output_limit_round_trip_in_bounded_chunks() {
    let api = MockFly::new();
    let bytes: Vec<u8> = (0..MAX_OUTPUT + 153).map(|i| (i % 256) as u8).collect();
    api.guest
        .lock()
        .unwrap()
        .files
        .insert("/work/existing.bin".into(), bytes.clone());
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    assert_eq!(
        fly.get_file(&lease, "/work/existing.bin")
            .await
            .unwrap()
            .as_ref(),
        bytes.as_slice()
    );
    fly.put_file(&lease, "/work/new.bin", Bytes::from(bytes.clone()))
        .await
        .unwrap();
    assert_eq!(
        fly.get_file(&lease, "/work/new.bin")
            .await
            .unwrap()
            .as_ref(),
        bytes.as_slice()
    );
    fly.put_file(&lease, "/work/new.bin", Bytes::new())
        .await
        .unwrap();
    assert!(fly
        .get_file(&lease, "/work/new.bin")
        .await
        .unwrap()
        .is_empty());
    assert!(api.guest.lock().unwrap().largest_stdin <= CHUNK_BYTES.div_ceil(3) * 4);
    fly.release(lease).await.unwrap();
}

#[tokio::test]
async fn changed_or_incomplete_transfers_never_report_success() {
    let api = MockFly::new();
    api.guest
        .lock()
        .unwrap()
        .files
        .insert("/work/file.bin".into(), vec![1; CHUNK_BYTES + 3]);
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    api.guest.lock().unwrap().corrupt_download = true;
    assert!(fly.get_file(&lease, "/work/file.bin").await.is_err());
    {
        let mut guest = api.guest.lock().unwrap();
        guest.corrupt_download = false;
        guest.truncate_download = true;
    }
    assert!(fly.get_file(&lease, "/work/file.bin").await.is_err());
    api.guest.lock().unwrap().truncate_upload = true;
    assert!(fly
        .put_file(
            &lease,
            "/work/file.bin",
            Bytes::from(vec![7; CHUNK_BYTES + 3])
        )
        .await
        .is_err());
    fly.release(lease).await.unwrap();
}

#[tokio::test]
async fn oversize_files_and_api_responses_are_refused() {
    let api = MockFly::new();
    api.guest
        .lock()
        .unwrap()
        .files
        .insert("/work/file.bin".into(), vec![1]);
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    assert!(matches!(
        fly.put_file(
            &lease,
            "/work/file.bin",
            Bytes::from(vec![0; MAX_FILE_BYTES + 1])
        )
        .await,
        Err(SandboxError::Unsupported(_))
    ));
    assert_eq!(api.guest.lock().unwrap().file_commands, 0);
    api.guest.lock().unwrap().oversized_file = true;
    assert!(matches!(
        fly.get_file(&lease, "/work/file.bin").await,
        Err(SandboxError::Unsupported(_))
    ));
    api.guest.lock().unwrap().oversized_response = true;
    let error = fly.state_of("machine-test").await.unwrap_err();
    assert!(error.to_string().contains("response exceeds 16 MiB"));
    fly.release(lease).await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_actual_guest_scripts_round_trip_binary_data_and_quoted_paths() {
    let api = MockFly::new();
    api.guest.lock().unwrap().real_exec = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("quoted 'name'; $(touch unexpected)\n.bin");
    let path = path.to_str().unwrap();
    let bytes: Vec<u8> = (0..MAX_OUTPUT + 153).map(|i| (i % 256) as u8).collect();
    let fly = api.provider();
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    fly.put_file(&lease, path, Bytes::from(bytes.clone()))
        .await
        .unwrap();
    assert_eq!(
        fly.get_file(&lease, path).await.unwrap().as_ref(),
        bytes.as_slice()
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    fly.release(lease).await.unwrap();
}
