//! Bounded headless component protocol; only actual GridCraft commands execute.
use gridcraft_engine::{Session, io};
use gridcraft_zoen_host::{Command, HostBridge, Open, Proposal};
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};
const MAX_DOCUMENT: usize = 512 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u8,
    open: Open,
    command: Option<Command>,
}
fn run() -> Result<Proposal, String> {
    let mut input = std::io::stdin().lock();
    let mut prefix = [0u8; 4];
    input.read_exact(&mut prefix).map_err(|_| "missing frame")?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length == 0 || length > 64 * 1024 {
        return Err("invalid metadata size".into());
    }
    let mut meta = vec![0; length];
    input
        .read_exact(&mut meta)
        .map_err(|_| "incomplete metadata")?;
    let request: Request = serde_json::from_slice(&meta).map_err(|_| "invalid request")?;
    if request.schema != 1 {
        return Err("unsupported schema".into());
    }
    let mut bytes = Vec::new();
    input
        .take((MAX_DOCUMENT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "incomplete document")?;
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT {
        return Err("document exceeds 512 KiB executor limit".into());
    }
    let mut session = Session::new();
    let mut bridge = HostBridge::default();
    bridge.open(&mut session, request.open, &bytes)?;
    if let Some(command) = request.command {
        let proposal = bridge.execute(&mut session, command)?;
        if proposal.bytes.len() > MAX_DOCUMENT {
            return Err("proposal exceeds 512 KiB executor limit".into());
        }
        Ok(proposal)
    } else {
        let value = session
            .execute("document.inspect", json!({}))
            .map_err(|e| e.to_string())?;
        Ok(Proposal {
            metadata: {
                let mut status = bridge.status(&session);
                status["workbook"] = value;
                status
            },
            bytes: Vec::new(),
        })
    }
}
fn output() -> Result<(), String> {
    // Explicit build-time fixture generation: actual upstream writer, not a template.
    if std::env::args().nth(1).as_deref() == Some("--seed") {
        let mut s = Session::new();
        s.new_workbook();
        let bytes = io::save_bytes(&s.doc().map_err(|e| e.to_string())?.wb, "Workbook.xlsx")
            .map_err(|e| e.to_string())?;
        gridcraft_zoen_host::profile::Document::parse(&bytes).map_err(str::to_owned)?;
        return std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| e.to_string());
    }
    let (envelope, bytes) = match run() {
        Ok(p) => (json!({"ok":true,"metadata":p.metadata}), p.bytes),
        Err(e) => (json!({"ok":false,"error":e}), Vec::new()),
    };
    let meta = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    if meta.len() > 64 * 1024 {
        return Err("response metadata exceeds 64 KiB".into());
    }
    let mut out = std::io::stdout().lock();
    out.write_all(&(meta.len() as u32).to_le_bytes())
        .and_then(|()| out.write_all(&meta))
        .and_then(|()| out.write_all(&bytes))
        .map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = output() {
        eprintln!("GridCraft worker: {error}");
        std::process::exit(1);
    }
}
