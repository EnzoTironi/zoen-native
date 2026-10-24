//! Same bounded LE metadata-length + JSON + raw document ABI as Deck/Grid workers.
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};
use wordcraft_zoen_host::{Command, HostBridge, Open, Proposal, profile};

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
        .take((profile::MAX_DOCUMENT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "incomplete document")?;
    let mut bridge = HostBridge::open(request.open, &bytes)?;
    match request.command {
        Some(command) => bridge.execute(command),
        None => Ok(Proposal {
            metadata: bridge.inspect(),
            bytes: Vec::new(),
        }),
    }
}

fn output() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("--seed") {
        return std::io::stdout()
            .lock()
            .write_all(&profile::encode("")?)
            .map_err(|_| "output failed".into());
    }
    let (envelope, bytes) = match run() {
        Ok(proposal) => (
            json!({"ok":true,"metadata":proposal.metadata}),
            proposal.bytes,
        ),
        Err(error) => (json!({"ok":false,"error":error}), Vec::new()),
    };
    let meta = serde_json::to_vec(&envelope).map_err(|_| "invalid response")?;
    let length = u32::try_from(meta.len()).map_err(|_| "response size")?;
    if meta.len() > 64 * 1024 {
        return Err("response metadata size".into());
    }
    let mut output = std::io::stdout().lock();
    output
        .write_all(&length.to_le_bytes())
        .and_then(|()| output.write_all(&meta))
        .and_then(|()| output.write_all(&bytes))
        .map_err(|_| "output failed".into())
}

fn main() {
    if let Err(error) = output() {
        eprintln!("WordCraft worker: {error}");
        std::process::exit(1);
    }
}
