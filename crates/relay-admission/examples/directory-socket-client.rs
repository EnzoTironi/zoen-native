//! Synthetic native client for the opt-in private remote Directory socket probe.
//! Reads one JSON command on stdin; secrets stay in an owned 0700 directory.
//! This example is not part of the Worker/WASM library or a provider-auth fixture.
use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{device_cert_message, media, sha256_hex, Signer};
use protocol::{
    auth_message, blob_put_message, ClientFrame, ErrorCode, Op, Reply, ServerFrame,
    PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use types::{Identity, IdentityKind};

fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str, Box<dyn Error>> {
    input[key]
        .as_str()
        .ok_or_else(|| format!("missing {key}").into())
}

fn write_secret(home: &Path, name: &str, signer: &Signer) -> Result<(), Box<dyn Error>> {
    write_private(home, name, &signer.secret())
}

fn write_private(home: &Path, name: &str, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(home.join(name))?.write_all(bytes)?;
    Ok(())
}

fn media_slot(input: &Value) -> Result<&'static str, Box<dyn Error>> {
    match text(input, "slot")? {
        "one" => Ok("one"),
        "two" => Ok("two"),
        _ => Err("unknown owned media slot".into()),
    }
}

fn media_fixture(home: &Path) -> Result<Value, Box<dyn Error>> {
    let identity = signer(home, "root.key")?.id();
    let mut objects = Vec::new();
    let mut total = 0;
    for (index, slot) in ["one", "two"].into_iter().enumerate() {
        let mut plain = format!("Zoen private media probe {slot}: {identity}\n").into_bytes();
        plain.extend((0..512 + index * 128).map(|value| value as u8));
        let (key, ciphertext) = media::seal(&plain);
        total += ciphertext.len();
        if total > 8192 {
            return Err("owned media budget exceeded".into());
        }
        write_private(home, &format!("media-{slot}.key"), &key)?;
        write_private(home, &format!("media-{slot}.plain"), &plain)?;
        write_private(home, &format!("media-{slot}.cipher"), &ciphertext)?;
        let sha = sha256_hex(&ciphertext);
        let object_key =
            relay_admission::blobs::get_key(&sha).map_err(|_| "invalid owned media digest")?;
        objects.push(json!({
            "slot":slot,"sha":sha,"object_key":object_key,
            "ciphertext_base64":STANDARD.encode(&ciphertext),"ciphertext_bytes":ciphertext.len(),
            "plaintext_sha256":sha256_hex(&plain),"plaintext_bytes":plain.len(),
        }));
    }
    Ok(json!({"schema":1,"objects":objects,"total_ciphertext_bytes":total}))
}

fn signer(home: &Path, name: &str) -> Result<Signer, Box<dyn Error>> {
    let bytes: [u8; 32] = fs::read(home.join(name))?
        .try_into()
        .map_err(|_| "invalid owned secret size")?;
    Ok(Signer::from_secret(&bytes))
}

fn fixture(root: &Signer, device: &Signer) -> Value {
    let identity = root.id();
    let profile = Identity {
        id: identity.clone(),
        kind: IdentityKind::Person,
        name: String::new(),
        handle: format!("qa_{}", &identity[..20]),
        tint_hex: "#415A77".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    };
    let encode = |frame: ServerFrame| STANDARD.encode(frame.encode());
    let request = |id, op| STANDARD.encode(ClientFrame::Req { id, op }.encode());
    json!({
        "schema":1,"identity":identity,"device":device.id(),"profile":profile,
        "hello_base64":STANDARD.encode(ClientFrame::Hello {
            protocol:PROTOCOL_VERSION,capabilities:Vec::new(),identity:root.id(),
            device:device.id(),cert:root.sign(&device_cert_message(&device.id())),
        }.encode()),
        "ready_new_base64":encode(ServerFrame::Ready {identity:root.id(),registered:false}),
        "ready_registered_base64":encode(ServerFrame::Ready {identity:root.id(),registered:true}),
        "register_base64":request(1,Op::Register {profile:profile.clone()}),
        "registered_base64":encode(ServerFrame::Res {id:1,result:Ok(Reply::Registered(profile.clone()))}),
        "stale_register_base64":request(2,Op::Register {profile}),
        "stale_reply_base64":encode(ServerFrame::Res {id:2,result:Err("account already registered; link this device first".into())}),
        "large_request_base64":request(9_007_199_254_740_993,Op::FetchLink {id:"0".repeat(64)}),
        "large_reply_base64":encode(ServerFrame::Res {id:9_007_199_254_740_993,result:Ok(Reply::Link(None))}),
        "ping_base64":STANDARD.encode(ClientFrame::Ping.encode()),
        "pong_base64":encode(ServerFrame::Pong {server_time_ms:None}),
        "unlink_base64":request(3,Op::Unlink {device:device.id()}),
        "unlink_done_base64":encode(ServerFrame::Res {id:3,result:Ok(Reply::Done)}),
        "revoked_error_base64":encode(ServerFrame::error(ErrorCode::Unauthorized,"this device was unlinked")),
        "expired_hello_error_base64":encode(ServerFrame::error(ErrorCode::Other,"expected hello")),
        "expired_auth_error_base64":encode(ServerFrame::error(ErrorCode::Other,"expected auth")),
    })
}

fn run() -> Result<Value, Box<dyn Error>> {
    let mut bytes = Vec::new();
    io::stdin().take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err("command too large".into());
    }
    let input: Value = serde_json::from_slice(&bytes)?;
    let home = Path::new(text(&input, "home")?);
    match text(&input, "op")? {
        "init" => {
            fs::create_dir(home)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(home, fs::Permissions::from_mode(0o700))?;
            }
            let root = Signer::generate();
            let device = Signer::generate();
            write_secret(home, "root.key", &root)?;
            write_secret(home, "device.key", &device)?;
            Ok(fixture(&root, &device))
        }
        "auth" => {
            let challenge = STANDARD.decode(text(&input, "challenge_base64")?)?;
            let frame = ServerFrame::decode(&challenge).map_err(|_| "invalid native challenge")?;
            if frame.encode() != challenge {
                return Err("noncanonical challenge".into());
            }
            let ServerFrame::Challenge {
                nonce,
                relay,
                protocol,
                capabilities,
                server_time_ms,
            } = frame
            else {
                return Err("expected native challenge".into());
            };
            if relay != "zoen-cloudflare-private"
                || protocol != PROTOCOL_VERSION
                || !capabilities.is_empty()
                || server_time_ms.is_some()
            {
                return Err("unexpected native challenge scope".into());
            }
            let device = signer(home, "device.key")?;
            Ok(json!({"auth_base64":STANDARD.encode(ClientFrame::Auth {
                sig:device.sign(&auth_message(&nonce,&relay)),
            }.encode())}))
        }
        "media-init" => media_fixture(home),
        "media-sign" => {
            let slot = media_slot(&input)?;
            let ciphertext = fs::read(home.join(format!("media-{slot}.cipher")))?;
            if ciphertext.len() > 8192 {
                return Err("owned media budget exceeded".into());
            }
            let sha = sha256_hex(&ciphertext);
            let timestamp: i64 = SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?;
            let device = signer(home, "device.key")?;
            let signature = device.sign(&blob_put_message(
                &sha,
                timestamp,
                "zoen-cloudflare-private",
            ));
            Ok(
                json!({"sha":sha,"device":device.id(),"timestamp":timestamp.to_string(),"signature":signature}),
            )
        }
        "media-open" => {
            let slot = media_slot(&input)?;
            let ciphertext = STANDARD.decode(text(&input, "ciphertext_base64")?)?;
            if ciphertext.len() > 8192 {
                return Err("owned media budget exceeded".into());
            }
            let key = fs::read(home.join(format!("media-{slot}.key")))?;
            let plain = media::open(&key, &ciphertext).ok_or("owned media did not decrypt")?;
            if plain != fs::read(home.join(format!("media-{slot}.plain")))? {
                return Err("owned media plaintext differs".into());
            }
            Ok(
                json!({"opened":true,"plaintext_sha256":sha256_hex(&plain),"plaintext_bytes":plain.len()}),
            )
        }
        _ => Err("unknown synthetic client command".into()),
    }
}

fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(_) => {
            // Never echo stdin, secret paths, keys, signatures or foreign errors.
            eprintln!("synthetic Directory client command failed");
            std::process::exit(1);
        }
    }
}
