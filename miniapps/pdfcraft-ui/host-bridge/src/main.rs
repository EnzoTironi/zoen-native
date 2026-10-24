//! Same bounded LE metadata-length + JSON + raw document ABI as Deck/Grid workers.
use pdfcraft_zoen_host::{Command, HostBridge, MAX_DOCUMENT_BYTES, Open, Proposal};
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u8,
    open: Open,
    command: Option<Command>,
    read: Option<ReadQuery>,
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
    if request.schema != 1 || (request.command.is_some() && request.read.is_some()) {
        return Err("unsupported schema".into());
    }
    let mut bytes = Vec::new();
    input
        .take((MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "incomplete document")?;
    let mut bridge = HostBridge::open(request.open, &bytes)?;
    match request.command {
        Some(command) => bridge.execute(command),
        None => Ok(Proposal {
            metadata: bridge.inspect_query(request.read.as_ref())?,
            bytes: Vec::new(),
        }),
    }
}

fn output() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("--seed") {
        return std::io::stdout()
            .lock()
            .write_all(&seed()?)
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

fn seed() -> Result<Vec<u8>, String> {
    let document = pdfcraft_create::blank(595.0, 842.0, 1).map_err(|_| "seed_creation")?;
    pdfcraft_cos::write_full(&document, &pdfcraft_cos::SaveOptions::default())
        .map_err(|_| "seed_encoding".into())
}

fn main() {
    if let Err(error) = output() {
        eprintln!("PDF worker: {error}");
        std::process::exit(1);
    }
}
