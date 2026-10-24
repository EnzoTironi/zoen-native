//! Local diagnostic: `zoen-relay log read SPACE | sequenced-metadata SPACE`.
//! Never persist/echo the upstream hex stream. Output is buffered until complete
//! validated EOF; the operator must also require the upstream process to succeed.
use std::io::{self, BufRead, Read};

use protocol::{SealedKind, Sequenced};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MAX_ROWS: usize = 64;
const MAX_WIRE_BYTES: usize = 98_304;
const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum Rejected {
    Io,
    Bounds,
    Truncated,
    Decode,
    Chain,
    Signature,
}

#[derive(Serialize)]
struct Row {
    seq: u64,
    kind: &'static str,
    is_pruned: bool,
    device_fingerprint: Option<String>,
}

#[derive(Serialize)]
struct Projection {
    // Verification is not proof of current membership, delivery or MLS readiness.
    identity_binding: &'static str,
    rows: Vec<Row>,
}

fn project(input: impl Read, space: &str) -> Result<Projection, Rejected> {
    let mut input = io::BufReader::new(input.take(MAX_INPUT_BYTES + 1));
    let mut total = 0_u64;
    let mut previous = types::GENESIS_PREV.to_owned();
    let mut rows = Vec::new();
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take((MAX_WIRE_BYTES * 2 + 2) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| Rejected::Io)?;
        if read == 0 {
            break;
        }
        total += u64::try_from(read).map_err(|_| Rejected::Bounds)?;
        if total > MAX_INPUT_BYTES || read > MAX_WIRE_BYTES * 2 + 1 || rows.len() == MAX_ROWS {
            return Err(Rejected::Bounds);
        }
        if line.pop() != Some(b'\n') {
            return Err(Rejected::Truncated);
        }
        let bytes = hex::decode(line).map_err(|_| Rejected::Decode)?;
        let entry = Sequenced::decode(&bytes).map_err(|_| Rejected::Decode)?;
        if entry.seq != u64::try_from(rows.len()).map_err(|_| Rejected::Bounds)?
            || entry.env.space() != space
            || entry.prev != previous
            || event_log::chain_hash(space, entry.seq, &entry.prev, &entry.env.wire_hash())?
                != entry.hash
        {
            return Err(Rejected::Chain);
        }
        entry.env.verify().map_err(|_| Rejected::Signature)?;
        let kind = match entry.env.sealed_kind() {
            Some(SealedKind::Application) => "sealed_application",
            Some(SealedKind::Commit) => "sealed_commit",
            Some(SealedKind::Welcome) => "sealed_welcome",
            Some(SealedKind::Unspecified) => "sealed_unknown",
            None => "plain",
        };
        rows.push(Row {
            seq: entry.seq,
            kind,
            is_pruned: entry.env.is_pruned(),
            device_fingerprint: entry
                .env
                .device()
                .map(|device| hex::encode(Sha256::digest(device.as_bytes()))),
        });
        previous = entry.hash;
    }
    if rows.is_empty() {
        return Err(Rejected::Truncated);
    }
    Ok(Projection {
        identity_binding: "not_checked",
        rows,
    })
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [space] = args.as_slice() else {
        eprintln!("usage: sequenced-metadata SPACE (private hex stdin; maximum 64 rows/8 MiB)");
        return std::process::ExitCode::FAILURE;
    };
    match project(io::stdin().lock(), space) {
        Ok(result) => match serde_json::to_string(&result) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(_) => {
                eprintln!("metadata rejected: output");
                std::process::ExitCode::FAILURE
            }
        },
        Err(reason) => {
            eprintln!("metadata rejected: {reason:?}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "sequenced-metadata/tests.rs"]
mod tests;
