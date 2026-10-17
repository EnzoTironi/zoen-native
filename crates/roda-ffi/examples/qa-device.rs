//! Disposable primary device for QA against an explicitly selected local relay.
//! It uses the real engine, signatures, registration, grants, linking and history.
//! No account, SQLite file or vault material survives this process.

use std::{
    collections::HashMap,
    io::{self, BufRead, Read, Write},
    net::IpAddr,
    sync::{Arc, Mutex},
};

use roda_ffi::{RodaEngine, SecretVault};
use serde_json::{json, Value};
use zeroize::Zeroize;

const MAX_LINE_BYTES: usize = 1 << 20;
const USAGE: &str = "qa-device --handle qa_<unique_test_name> --relay http://127.0.0.1:8787\n\
Reads one canonical JSON command per line; EOF stops sync and discards the device.\n\
Only loopback HTTP address literals and synthetic qa_ handles are accepted.";

#[derive(Default)]
struct VolatileVault(Mutex<HashMap<String, Vec<u8>>>);

impl SecretVault for VolatileVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }

    fn save(&self, key: String, value: Vec<u8>) -> bool {
        let Ok(mut values) = self.0.lock() else {
            return false;
        };
        if let Some(mut previous) = values.insert(key, value) {
            previous.zeroize();
        }
        true
    }

    fn delete(&self, key: String) {
        if let Ok(mut values) = self.0.lock() {
            if let Some(mut previous) = values.remove(&key) {
                previous.zeroize();
            }
        }
    }
}

impl Drop for VolatileVault {
    fn drop(&mut self) {
        if let Ok(values) = self.0.get_mut() {
            for value in values.values_mut() {
                value.zeroize();
            }
            values.clear();
        }
    }
}

fn loopback_relay(input: &str) -> Result<String, &'static str> {
    let url = reqwest::Url::parse(input).map_err(|_| "Invalid relay URL.")?;
    let host = url
        .host_str()
        .ok_or("A loopback address literal is required.")?;
    let address: IpAddr = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .map_err(|_| "Use a loopback address literal, for example 127.0.0.1 or [::1].")?;
    if input.trim() != input
        || url.scheme() != "http"
        || !address.is_loopback()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port() == Some(0)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("The QA relay must be a credential-free loopback HTTP origin.");
    }
    Ok(url.origin().ascii_serialization())
}

fn synthetic_handle(value: &str) -> bool {
    value.starts_with("qa_")
        && (4..=24).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
        })
}

fn options(args: impl IntoIterator<Item = String>) -> Result<(String, String), &'static str> {
    let mut args = args.into_iter();
    let mut handle = None;
    let mut relay = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("Every option requires a value.")?;
        match flag.as_str() {
            "--handle" if handle.is_none() => handle = Some(value),
            "--relay" if relay.is_none() => relay = Some(loopback_relay(&value)?),
            _ => return Err("Unknown or duplicate option."),
        }
    }
    let handle = handle.ok_or("An explicit synthetic --handle is required.")?;
    if !synthetic_handle(&handle) {
        return Err("Use a unique lowercase qa_ handle of 4 to 24 ASCII characters.");
    }
    Ok((
        handle,
        relay.ok_or("An explicit loopback --relay is required.")?,
    ))
}

/// This fixture forwards real operations, not an engine/storage bypass. Exclude
/// social auth and recovery/key operations so credentials can never become output.
fn allowed_operation(value: &Value) -> bool {
    let Some(op) = value.get("op").and_then(Value::as_str) else {
        return false;
    };
    matches!(
        op,
        "snapshot"
            | "devices"
            | "link_code_check"
            | "link_device"
            | "link_progress"
            | "send_history"
            | "history_status"
            | "find_people"
            | "start_direct"
            | "create_group"
            | "create_community"
            | "create_invite"
            | "preview_invite"
            | "join_invite"
            | "add_member"
            | "add_admin"
            | "remove_member"
            | "leave_space"
            | "encrypt_chat"
            | "set_chat_prefs"
            | "timeline"
            | "message"
            | "thread"
            | "send_message"
            | "edit_message"
            | "delete_message"
            | "react"
            | "forward_message"
            | "mark_read"
            | "items"
            | "item"
            | "item_at"
            | "page"
            | "page_at"
            | "page_create"
            | "page_import_markdown"
            | "page_parse_markdown"
            | "page_block_markdown"
            | "new_mutation_id"
            | "page_apply"
            | "page_commit"
            | "page_markdown"
            | "app_specs"
            | "install_app"
            | "app_call_tool"
            | "app_resource"
            | "profile"
            | "search"
            | "search_messages"
            | "universal_search"
            | "verify_log"
            | "log_events"
    )
}

fn emit(value: Value) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &value)?;
    writeln!(output)?;
    output.flush()
}

async fn run(handle: String, relay: String) -> Result<(), Box<dyn std::error::Error>> {
    let vault = Arc::new(VolatileVault::default());
    let core = RodaEngine::open(":memory:".into(), "en-US".into())?;
    core.create_account("QA disposable device".into(), handle, relay, vault.clone())?;
    core.start_sync(None)?;
    if core.wait_until_settled(20_000).await.is_err() {
        core.stop_sync();
        return Err("The disposable account did not register and sync with the local relay. Use a running relay and a fresh qa_ handle.".into());
    }
    emit(json!({ "event": "ready", "disposable": true, "account": core.account() }))?;

    let mut input = io::stdin().lock();
    loop {
        let mut bytes = Vec::new();
        let length = (&mut input)
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut bytes)?;
        if length == 0 {
            break;
        }
        if length > MAX_LINE_BYTES {
            emit(
                json!({ "ok": false, "error": "Command exceeds the 1 MiB limit; closing the disposable fixture." }),
            )?;
            break;
        }
        if bytes.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let command = serde_json::from_slice::<Value>(&bytes);
        if !command.as_ref().is_ok_and(allowed_operation) {
            emit(
                json!({ "ok": false, "error": "Invalid or unavailable QA operation. Social auth, vault and recovery operations are excluded." }),
            )?;
            continue;
        }
        let request = String::from_utf8(bytes)?;
        match core.client_vault_request(request, vault.clone()).await {
            Ok(result) => {
                // Keep the core's wire JSON intact instead of round-tripping IDs,
                // prices or other precision-sensitive values through another DTO.
                let mut output = io::stdout().lock();
                writeln!(output, "{{\"ok\":true,\"result\":{result}}}")?;
                output.flush()?;
            }
            Err(_) => emit(
                json!({ "ok": false, "error": "The canonical operation failed; check the request and local relay." }),
            )?,
        }
    }
    core.stop_sync();
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("{USAGE}");
        return;
    }
    let result = match options(args) {
        Ok((handle, relay)) => run(handle, relay).await,
        Err(reason) => {
            eprintln!("{reason}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if result.is_err() {
        // Never print an underlying command, vault value or provider credential.
        eprintln!(
            "QA device stopped before readiness. Check the loopback relay and use a fresh qa_ handle."
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_requires_explicit_loopback_origin_and_synthetic_account() {
        assert_eq!(
            loopback_relay("http://127.0.0.1:8787").unwrap(),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            loopback_relay("http://[::1]:8787").unwrap(),
            "http://[::1]:8787"
        );
        for relay in [
            "https://127.0.0.1:8787",
            "http://relay.tryzoen.com",
            "http://localhost:8787",
            "http://192.168.1.2:8787",
            "http://user:password@127.0.0.1:8787",
            "http://127.0.0.1:8787/path",
            "http://127.0.0.1:8787?token=x",
        ] {
            assert!(loopback_relay(relay).is_err());
        }
        assert!(options(Vec::new()).is_err());
        assert!(!synthetic_handle("real_person"));
        assert!(synthetic_handle("qa_ios_pair_1"));
    }

    #[test]
    fn fixture_forwards_pairing_and_messages_but_never_auth_or_recovery_secrets() {
        assert!(allowed_operation(
            &json!({"op":"link_device","code":"typed in the UI"})
        ));
        assert!(allowed_operation(&json!({"op":"send_message"})));
        for op in [
            "provider_sign_in",
            "backup_create_recovery_key",
            "backup_enable_recovery_key",
            "backup_password",
            "restore_backup_password",
            "restore_backup_recovery_key",
            "link_request",
        ] {
            assert!(!allowed_operation(&json!({ "op": op })));
        }
    }
}
