//! Fail-only, allowlisted observations of synthetic journey readiness.
//! Never retains a home, protocol payload, argument, URL or raw process output.

use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Output,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

const MAX_RECORD: usize = 2_048;
const MAX_DEADLINES: usize = 16;
const DEADLINE_PREFIX: &[u8] = b"{\"event\":\"zoen_sync_wait_deadline\",";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Offline,
    Connecting,
    Online,
    Unparsed,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Deadline {
    event: DeadlineEvent,
    version: u8,
    timeout_ms: u64,
    elapsed_ms: u64,
    state: State,
    synced: bool,
    pending: u64,
    #[serde(deserialize_with = "required_flag")]
    unknown_ready: Option<bool>,
    #[serde(deserialize_with = "required_flag")]
    uploads_ready: Option<bool>,
    #[serde(deserialize_with = "required_flag")]
    profiles_ready: Option<bool>,
    #[serde(deserialize_with = "required_flag")]
    mls_ready: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize)]
enum DeadlineEvent {
    #[serde(rename = "zoen_sync_wait_deadline")]
    Deadline,
}

fn required_flag<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Option<bool>, D::Error> {
    Option::<bool>::deserialize(input)
}

impl Deadline {
    fn valid(&self) -> bool {
        if self.version != 1 || self.elapsed_ms < self.timeout_ms {
            return false;
        }
        let mut stopped = self.state != State::Online || !self.synced || self.pending != 0;
        for flag in [
            self.unknown_ready,
            self.uploads_ready,
            self.profiles_ready,
            self.mls_ready,
        ] {
            if stopped {
                if flag.is_some() {
                    return false;
                }
            } else {
                match flag {
                    Some(true) => {}
                    Some(false) => stopped = true,
                    None => return false,
                }
            }
        }
        stopped
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct SyncResult {
    event: &'static str,
    phase: &'static str,
    state: State,
    synced: Option<bool>,
    pending: Option<u64>,
    process_success: bool,
}

pub(super) fn parse_sync(bytes: &[u8], success: bool) -> SyncResult {
    let unparsed = || SyncResult {
        event: "zoen_journey_sync_result",
        phase: "initial",
        state: State::Unparsed,
        synced: None,
        pending: None,
        process_success: success,
    };
    if bytes.len() > 1_024 {
        return unparsed();
    }
    let Ok(line) = std::str::from_utf8(bytes) else {
        return unparsed();
    };
    let line = line.strip_suffix('\n').unwrap_or(line);
    if line.contains(['\r', '\n']) {
        return unparsed();
    }
    let Some(rest) = line.strip_prefix("connection=") else {
        return unparsed();
    };
    let Some((state, rest)) = rest.split_once(" synced=") else {
        return unparsed();
    };
    let state = match state {
        "offline" => State::Offline,
        "connecting" => State::Connecting,
        "online" => State::Online,
        _ => return unparsed(),
    };
    let Some((synced, rest)) = rest.split_once(" pending=") else {
        return unparsed();
    };
    let synced = match synced {
        "true" => true,
        "false" => false,
        _ => return unparsed(),
    };
    // The existing optional error suffix is deliberately discarded, never copied.
    let pending = rest
        .split_once(" error=")
        .map_or(rest, |(pending, _)| pending);
    if pending.is_empty() || !pending.bytes().all(|b| b.is_ascii_digit()) {
        return unparsed();
    }
    let Ok(pending) = pending.parse() else {
        return unparsed();
    };
    SyncResult {
        event: "zoen_journey_sync_result",
        phase: "initial",
        state,
        synced: Some(synced),
        pending: Some(pending),
        process_success: success,
    }
}

#[derive(Clone, Default)]
struct Captured {
    first_sync: Option<SyncResult>,
    last_sync: Option<SyncResult>,
    deadlines: Vec<Deadline>,
    rejected_deadlines: u64,
    discarded_deadlines: u64,
}

pub(super) struct JourneyDiagnostics {
    directory: Option<PathBuf>,
    captured: Mutex<Captured>,
}

impl JourneyDiagnostics {
    pub(super) fn from_env() -> Self {
        Self::at(std::env::var_os("ZOEN_SYNC_DIAGNOSTIC_DIR").map(PathBuf::from))
    }

    pub(super) fn at(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            captured: Mutex::new(Captured::default()),
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.directory.is_some()
    }

    pub(super) fn observe(&self, args: &[&str], output: &Output) {
        if !self.enabled() {
            return;
        }
        let mut captured = self.captured.lock().unwrap_or_else(|p| p.into_inner());
        let is_sync = matches!(args, ["sync"])
            || matches!(args, ["--timeout", ms, "sync"] if ms.parse::<u64>().is_ok());
        if is_sync {
            let mut result = parse_sync(&output.stdout, output.status.success());
            if captured.first_sync.is_none() {
                captured.first_sync = Some(result.clone());
            }
            result.phase = "last";
            captured.last_sync = Some(result);
        }
        for line in output.stderr.split(|b| *b == b'\n') {
            if !line.starts_with(DEADLINE_PREFIX) {
                continue;
            }
            let record = (line.len() <= MAX_RECORD)
                .then(|| serde_json::from_slice::<Deadline>(line).ok())
                .flatten()
                .filter(Deadline::valid);
            if let Some(record) = record {
                if captured.deadlines.len() == MAX_DEADLINES {
                    captured.deadlines.remove(0);
                    captured.discarded_deadlines = captured.discarded_deadlines.saturating_add(1);
                }
                captured.deadlines.push(record);
            } else {
                captured.rejected_deadlines = captured.rejected_deadlines.saturating_add(1);
            }
        }
    }

    pub(super) fn publish_failure(&self) {
        let Some(directory) = &self.directory else {
            return;
        };
        let captured = self
            .captured
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for record in [captured.first_sync, captured.last_sync]
            .into_iter()
            .flatten()
        {
            write_record(directory, &record);
        }
        for record in captured.deadlines {
            write_record(directory, &record);
        }
        #[derive(Serialize)]
        struct Summary {
            event: &'static str,
            rejected_deadlines: u64,
            discarded_deadlines: u64,
        }
        write_record(
            directory,
            &Summary {
                event: "zoen_journey_sync_diagnostics",
                rejected_deadlines: captured.rejected_deadlines,
                discarded_deadlines: captured.discarded_deadlines,
            },
        );
    }
}

fn write_record(directory: &Path, value: &impl Serialize) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    if bytes.len() >= MAX_RECORD || !directory.is_absolute() {
        return;
    }
    let Ok(metadata) = std::fs::symlink_metadata(directory) else {
        return;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return;
    }
    let name = format!(
        "zoen-sync-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let _ = write_new_record(&directory.join(name), &bytes);
}

pub(super) fn write_new_record(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if bytes.len() >= MAX_RECORD {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.write_all(b"\n")
}
