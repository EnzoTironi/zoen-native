//! Synthetic native client for the opt-in private remote Directory socket probe.
//! Reads one JSON command on stdin; secrets stay in an owned 0700 directory.
//! This example is not part of the Worker/WASM library or a provider-auth fixture.
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_log::{device_cert_message, Signer};
use roda_proto::{auth_message, ClientFrame, ErrorCode, Op, Reply, ServerFrame, PROTOCOL_VERSION};
use roda_types::{Identity, IdentityKind};
use serde_json::{json, Value};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str, Box<dyn Error>> {
    input[key]
        .as_str()
        .ok_or_else(|| format!("missing {key}").into())
}

fn write_secret(home: &Path, name: &str, signer: &Signer) -> Result<(), Box<dyn Error>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(home.join(name))?.write_all(&signer.secret())?;
    Ok(())
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
