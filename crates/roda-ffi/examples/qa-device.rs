//! Disposable primary or identity-pinned linked device for an explicit local QA relay.
//! It uses the real engine, signatures, registration, grants, linking and history.
//! No account, SQLite file or vault material survives this process.

use std::{
    collections::HashMap,
    io::{self, BufRead, Read, Write},
    net::IpAddr,
    sync::{Arc, Mutex},
};

use roda_ffi::{PhotoChange, RodaEngine, SecretVault};
use serde_json::{json, Value};
use zeroize::Zeroize;

const MAX_LINE_BYTES: usize = 1 << 20;
const USAGE: &str = "qa-device --handle qa_<unique_test_name> --relay http://127.0.0.1:8787 [--mode linked --expected-identity <public-id>]\n\
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

fn options(
    args: impl IntoIterator<Item = String>,
) -> Result<(String, String, bool, Option<String>), &'static str> {
    let mut args = args.into_iter();
    let mut handle = None;
    let mut relay = None;
    let mut linked = false;
    let mut expected_identity = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("Every option requires a value.")?;
        match flag.as_str() {
            "--handle" if handle.is_none() => handle = Some(value),
            "--relay" if relay.is_none() => relay = Some(loopback_relay(&value)?),
            "--mode" if value == "linked" && !linked => linked = true,
            "--expected-identity"
                if expected_identity.is_none() && roda_log::parse_device_key(&value).is_some() =>
            {
                expected_identity = Some(value)
            }
            _ => return Err("Unknown or duplicate option."),
        }
    }
    let handle = handle.ok_or("An explicit synthetic --handle is required.")?;
    if !synthetic_handle(&handle) {
        return Err("Use a unique lowercase qa_ handle of 4 to 24 ASCII characters.");
    }
    if linked != expected_identity.is_some() {
        return Err("Linked QA mode requires an explicit expected identity.");
    }
    Ok((
        handle,
        relay.ok_or("An explicit loopback --relay is required.")?,
        linked,
        expected_identity,
    ))
}

// Existing E2E-generated gradient JPEG: public synthetic pixels, fixed at compile time.
// There is deliberately no caller-supplied path, URI, MIME or photo byte argument.
const PROFILE_PHOTO: &[u8] = include_bytes!("../../../packages/zoen-ui/web/e2e/fixtures/praia.jpg");

fn qa_photo_operation(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| {
        fields.len() == 1
            && fields.get("op").and_then(Value::as_str) == Some("qa_set_profile_photo")
    })
}

fn set_fixture_profile_photo(
    core: &RodaEngine,
    identity: &str,
    device: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let account = core
        .account()
        .ok_or("The disposable account is unavailable.")?;
    if account.identity_id != identity
        || account.device_id != device
        || !synthetic_handle(&account.handle)
        || !account.unlocked
        || account.setup_pending
    {
        return Err("The disposable account context changed.".into());
    }
    let profile = core.update_my_profile(
        "QA reverse profile photo".into(),
        "Synthetic profile convergence QA.".into(),
        PhotoChange::Set {
            bytes: PROFILE_PHOTO.to_vec(),
            mime: "image/jpeg".into(),
        },
    )?;
    Ok(json!({ "version": profile.version.to_string(),
        "photo_sha256": profile.photo_sha256, "photo_ready": profile.photo_ready }))
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
            | "receive_history"
            | "load_older"
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
            | "plan_page"
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

async fn run(
    handle: String,
    relay: String,
    linked: bool,
    expected: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let vault = Arc::new(VolatileVault::default());
    let core = RodaEngine::open(":memory:".into(), "en-US".into())?;
    if linked {
        let request = core.link_request(relay, vault.clone())?;
        emit(json!({"event":"pending","request":request}))?;
        let request =
            json!({"op":"link_wait","timeout_ms":120_000,"expected_identity_id":expected})
                .to_string();
        core.client_vault_request(request, vault.clone()).await?;
        if !core
            .account()
            .is_some_and(|account| synthetic_handle(&account.handle))
        {
            return Err("The linked source is not a synthetic QA account.".into());
        }
    } else {
        core.create_account("QA disposable device".into(), handle, relay, vault.clone())?;
    }
    core.start_sync(None)?;
    if core.wait_until_settled(20_000).await.is_err() {
        core.stop_sync();
        return Err("The disposable account did not register and sync with the local relay. Use a running relay and a fresh qa_ handle.".into());
    }
    let owned_account = core
        .account()
        .ok_or("The disposable account is unavailable.")?;
    emit(json!({ "event": "ready", "disposable": true, "account": &owned_account }))?;

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
        if command.as_ref().is_ok_and(qa_photo_operation) {
            match set_fixture_profile_photo(
                &core,
                &owned_account.identity_id,
                &owned_account.device_id,
            ) {
                Ok(result) => emit(json!({ "ok": true, "result": result }))?,
                Err(_) => emit(
                    json!({ "ok": false, "error": "The synthetic profile photo operation failed." }),
                )?,
            }
            continue;
        }
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
        Ok((handle, relay, linked, expected)) => run(handle, relay, linked, expected).await,
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
    fn photo_operation_has_no_arbitrary_inputs_and_uses_a_fixed_bounded_jpeg() {
        assert!(qa_photo_operation(&json!({"op":"qa_set_profile_photo"})));
        for value in [
            json!({"op":"qa_set_profile_photo","path":"/private/photo.jpg"}),
            json!({"op":"qa_set_profile_photo","bytes":[1,2]}),
            json!({"op":"qa_set_profile_photo","identity":"another"}),
            json!({"op":"update_profile"}),
            Value::Null,
        ] {
            assert!(!qa_photo_operation(&value));
        }
        assert!(PROFILE_PHOTO.len() < 5 * 1024 * 1024);
        assert!(PROFILE_PHOTO.starts_with(&[0xff, 0xd8]));
        assert!(PROFILE_PHOTO.ends_with(&[0xff, 0xd9]));
        use sha2::{Digest, Sha256};
        assert_eq!(
            hex::encode(Sha256::digest(PROFILE_PHOTO)),
            "9d1e61801b349f3308d543998db013ff8220e713fd5977ad35f8ae7844fb1b56"
        );
    }

    #[test]
    fn photo_operation_uses_canonical_profile_and_refuses_another_owner() {
        let core = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
        core.create_account(
            "QA disposable device".into(),
            "qa_photo_boundary".into(),
            "http://127.0.0.1:9".into(),
            Arc::new(VolatileVault::default()),
        )
        .unwrap();
        let account = core.account().unwrap();
        assert!(set_fixture_profile_photo(&core, "different", &account.device_id).is_err());
        assert!(core
            .get_profile(account.identity_id.clone())
            .unwrap()
            .photo_sha256
            .is_none());
        let result =
            set_fixture_profile_photo(&core, &account.identity_id, &account.device_id).unwrap();
        assert_eq!(result["photo_ready"], true);
        assert_eq!(result.as_object().unwrap().len(), 3);
        let hash = result["photo_sha256"].as_str().unwrap();
        let photo = core
            .profile_photo(account.identity_id, hash.into())
            .unwrap()
            .unwrap();
        assert_eq!(photo.mime, "image/jpeg");
        assert_eq!(photo.bytes, PROFILE_PHOTO);
    }

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
    fn linked_fixture_requires_a_valid_identity_pin_and_retains_synthetic_scope() {
        let mut args = vec![
            "--handle".into(),
            "qa_linked".into(),
            "--relay".into(),
            "http://127.0.0.1:8790".into(),
        ];
        args.extend(["--mode".into(), "linked".into()]);
        assert!(options(args.clone()).is_err());
        let identity = roda_log::Signer::from_secret(&[91; 32]).id();
        args.extend(["--expected-identity".into(), identity.clone()]);
        let (handle, _, linked, expected) = options(args).unwrap();
        assert_eq!(handle, "qa_linked");
        assert!(linked);
        assert_eq!(expected, Some(identity.clone()));
        let without_mode = vec![
            "--handle".into(),
            "qa_linked".into(),
            "--relay".into(),
            "http://127.0.0.1:8790".into(),
            "--expected-identity".into(),
            identity,
        ];
        assert!(options(without_mode).is_err());
    }

    #[test]
    fn fixture_forwards_pairing_and_messages_but_never_auth_or_recovery_secrets() {
        assert!(allowed_operation(
            &json!({"op":"link_device","code":"typed in the UI"})
        ));
        assert!(allowed_operation(&json!({"op":"send_message"})));
        assert!(allowed_operation(&json!({"op":"receive_history"})));
        assert!(allowed_operation(&json!({"op":"load_older"})));
        assert!(allowed_operation(
            &json!({"op":"plan_page","item_id":"synthetic"})
        ));
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
