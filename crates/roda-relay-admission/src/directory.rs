//! Private directory policy. The host owns challenge entropy, one-use/session storage,
//! and a single transaction covering all facts and returned writes. No delivery lease
//! or public protocol endpoint is provided by this module.
pub mod key_packages;
pub mod reads;
pub mod socket;
use crate::portable::{BoundaryError, MAX_FRAME_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_proto::{auth_message, normalize_handle, ClientFrame, Op, Reply, ServerFrame};
use roda_types::{Identity, IdentityKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_LINK_BOX: usize = 64 * 1024;

pub fn certificate_valid(identity: &str, device: &str, cert: &str) -> bool {
    roda_log::verify_sig(identity, &roda_log::device_cert_message(device), cert)
}

pub fn enrollment_certificate_valid(identity: &str, device: &str, cert: &str) -> bool {
    valid_hex64(device) && certificate_valid(identity, device, cert)
}

pub fn auth_valid(device: &str, nonce: &str, relay: &str, sig: &str) -> bool {
    roda_log::verify_sig(device, &auth_message(nonce, relay), sig)
}

pub fn provisional_allowed(
    enrolled: Option<(bool, bool)>,
    identity: &str,
    device: &str,
    require_device: bool,
) -> bool {
    !require_device
        && enrolled.is_none_or(|(_, revoked)| !revoked)
        && (enrolled.is_none() || identity == device)
}

pub fn request_authorized(enrolled: Option<(bool, bool)>, unregistered: bool) -> bool {
    enrolled == Some((true, false)) || unregistered
}

/// Exact native session profile normalization, shared before persistence.
pub fn prepare_profile(mut profile: Identity, identity: &str) -> Result<Identity, String> {
    if profile.id != identity {
        return Err("profile is for another identity".into());
    }
    profile.handle = normalize_handle(&profile.handle)
        .ok_or("handle must be 3–24 letters, digits, '.' or '_', starting with a letter")?;
    match profile.kind {
        IdentityKind::Person => {
            profile.name.clear();
            profile.bio.clear();
        }
        IdentityKind::Agent => {
            profile.name = profile.name.trim().chars().take(64).collect();
            profile.bio = profile.bio.chars().take(280).collect();
            if profile.name.is_empty() {
                return Err("name is required".into());
            }
            if profile.owner.is_none() {
                return Err("an agent needs an owner".into());
            }
        }
    }
    Ok(profile)
}

pub fn enrollment_matches(existing: &Device, identity: &str, cert: &str) -> bool {
    existing.identity == identity && existing.cert == cert && !existing.revoked
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub identity: String,
    pub cert: String,
    pub revoked: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol: u32,
    pub capabilities: Vec<String>,
    pub identity: String,
    pub device: String,
    pub cert: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub identity: String,
    pub device: String,
    pub cert: String,
    pub registered: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub registered: bool,
    pub device: Option<Device>,
    pub target: Option<Device>,
    pub profile: Option<Identity>,
    pub handle_owner: Option<String>,
    pub link_box: Option<Vec<u8>>,
    #[serde(default)]
    pub key_packages: Option<key_packages::Facts>,
    #[serde(default)]
    pub reads: Option<reads::Facts>,
}

fn no(reason: impl Into<String>) -> BoundaryError {
    BoundaryError::no("directory", reason)
}
fn internal(reason: impl Into<String>) -> BoundaryError {
    BoundaryError::internal("invalid_directory_context", reason)
}
fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn decode(bytes: &[u8]) -> Result<ClientFrame, BoundaryError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(no("invalid directory frame size"));
    }
    let frame = ClientFrame::decode(bytes).map_err(|_| no("invalid directory frame"))?;
    if frame.encode() != bytes {
        return Err(no("directory frame is not canonical"));
    }
    Ok(frame)
}
fn parse<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, BoundaryError> {
    if input.len() > MAX_FRAME_BYTES {
        return Err(internal("directory context too large"));
    }
    serde_json::from_str(input).map_err(|_| internal("invalid directory context"))
}
fn encoded(value: &impl Serialize) -> Result<String, BoundaryError> {
    serde_json::to_string(value).map_err(|_| internal("directory encoding failed"))
}

pub fn hello(bytes: &[u8]) -> Result<Hello, BoundaryError> {
    let ClientFrame::Hello {
        protocol,
        capabilities,
        identity,
        device,
        cert,
    } = decode(bytes)?
    else {
        return Err(no("expected hello"));
    };
    if protocol < roda_proto::MIN_PROTOCOL_VERSION {
        return Err(no(format!(
            "protocol {protocol} is too old, this relay needs {} or newer",
            roda_proto::MIN_PROTOCOL_VERSION
        )));
    }
    if !certificate_valid(&identity, &device, &cert) {
        return Err(no("device certificate invalid"));
    }
    Ok(Hello {
        protocol,
        capabilities: roda_proto::negotiate(&capabilities),
        identity,
        device,
        cert,
    })
}

pub fn authenticate(
    bytes: &[u8],
    hello: &Hello,
    nonce: &str,
    relay: &str,
    facts: &Facts,
) -> Result<Session, BoundaryError> {
    let ClientFrame::Auth { sig } = decode(bytes)? else {
        return Err(no("expected auth"));
    };
    if !auth_valid(&hello.device, nonce, relay, &sig) {
        return Err(no("login signature invalid"));
    }
    if facts.device.as_ref().is_some_and(|row| row.revoked) {
        return Err(no("this device was unlinked"));
    }
    if facts.registered
        && !facts
            .device
            .as_ref()
            .is_some_and(|row| enrollment_matches(row, &hello.identity, &hello.cert))
    {
        return Err(no("link this device first"));
    }
    let session = Session {
        identity: hello.identity.clone(),
        device: hello.device.clone(),
        cert: hello.cert.clone(),
        registered: facts.registered,
    };
    // The native session touches the exact stored certificate and takes its first
    // admission fence before Ready; a valid root signature alone is insufficient.
    authorize(&session, facts)?;
    Ok(session)
}

pub fn authorize(session: &Session, facts: &Facts) -> Result<(), BoundaryError> {
    let row = facts
        .device
        .as_ref()
        .map(|row| (row.identity == session.identity, row.revoked));
    let provisional =
        provisional_allowed(row, &session.identity, &session.device, session.registered);
    if request_authorized(row, provisional && !facts.registered) {
        Ok(())
    } else {
        Err(no("this device was unlinked"))
    }
}

pub fn lookup(bytes: &[u8]) -> Result<Value, BoundaryError> {
    let ClientFrame::Req { id, op } = decode(bytes)? else {
        return Err(no("expected request"));
    };
    let charge_lookup = reads::spends_lookup_budget(&op);
    if let Some(read) = reads::query(&op) {
        return Ok(
            json!({"id":id.to_string(),"kind":"directory_read","read":read,"charge_lookup":charge_lookup}),
        );
    }
    let (target, handle, link_id) = match op {
        Op::Register { profile } => (None, normalize_handle(&profile.handle), None),
        Op::DeliverLink { id, device, .. } => (device.map(|value| value.device), None, Some(id)),
        Op::Unlink { device } => (Some(device), None, None),
        Op::FetchLink { id } => (None, None, Some(id)),
        Op::PublishKeyPackages { packages, .. } => {
            let hashes: Vec<_> = if packages.len() <= key_packages::MAX_PUBLICATIONS
                && packages
                    .iter()
                    .all(|bytes| bytes.len() <= key_packages::MAX_PACKAGE_BYTES)
            {
                packages
                    .iter()
                    .map(|bytes| roda_log::sha256_hex(bytes))
                    .collect()
            } else {
                Vec::new()
            };
            return Ok(
                json!({"id":id.to_string(),"kind":"publish_key_packages","publication_hashes":hashes}),
            );
        }
        Op::ClaimKeyPackages { ids, operation_id } => {
            let targets = match &operation_id {
                Some(operation) => {
                    key_packages::claim_input(operation, &ids)
                        .map_err(no)?
                        .targets
                }
                None => ids.into_iter().take(50).collect(),
            };
            return Ok(
                json!({"id":id.to_string(),"kind":"claim_key_packages","targets":targets,"operation_id":operation_id}),
            );
        }
        _ => {
            return Err(BoundaryError::no(
                "unsupported_directory",
                "directory operation is outside the private adapter scope",
            ))
        }
    };
    Ok(
        json!({"id":id.to_string(),"target_device":target,"handle":handle,"link_id":link_id,"charge_lookup":charge_lookup}),
    )
}

pub fn lookup_for_session(bytes: &[u8], session: &Session) -> Result<Value, BoundaryError> {
    if !session.registered
        && matches!(
            decode(bytes)?,
            ClientFrame::Req {
                op: Op::PublishKeyPackages { .. }
                    | Op::ClaimKeyPackages { .. }
                    | Op::Lookup { .. }
                    | Op::Profiles { .. }
                    | Op::Devices,
                ..
            }
        )
    {
        return Err(no("register first"));
    }
    lookup(bytes)
}

pub fn request(bytes: &[u8], session: &Session, facts: &Facts) -> Result<Value, BoundaryError> {
    authorize(session, facts)?;
    let ClientFrame::Req { id, op } = decode(bytes)? else {
        return Err(no("expected request"));
    };
    let (mut plan, reply) = match op {
        Op::Register { profile } => {
            let profile = prepare_profile(profile, &session.identity).map_err(no)?;
            if !roda_log::agent_owner::profile_authorized(&profile) {
                return Err(no("profile ownership authorization invalid"));
            }
            if profile.kind != IdentityKind::Person {
                return Err(BoundaryError::no(
                    "unsupported_directory",
                    "agent directory registration is outside the private adapter scope",
                ));
            }
            if !session.registered {
                if facts.registered {
                    return Err(no("account already registered; link this device first"));
                }
                // The native INSERT claims the unique handle before enrollment.
                if facts
                    .handle_owner
                    .as_ref()
                    .is_some_and(|owner| owner != &profile.id)
                {
                    return Err(no("handle_taken"));
                }
                if !enrollment_certificate_valid(&profile.id, &session.device, &session.cert) {
                    return Err(no("device certificate invalid"));
                }
                if facts
                    .device
                    .as_ref()
                    .is_some_and(|row| !enrollment_matches(row, &profile.id, &session.cert))
                {
                    return Err(no("device is revoked or belongs to another account"));
                }
            } else {
                let stored = facts
                    .profile
                    .as_ref()
                    .ok_or_else(|| no("account isn't registered"))?;
                if stored.kind != profile.kind || stored.owner != profile.owner {
                    return Err(no("identity kind and owner cannot change"));
                }
                if stored.owner_proof != profile.owner_proof {
                    return Err(no("agent ownership proof cannot change"));
                }
            }
            if facts
                .handle_owner
                .as_ref()
                .is_some_and(|owner| owner != &profile.id)
            {
                return Err(no("handle_taken"));
            }
            (
                json!({"action":"register","first":!session.registered,"profile":profile}),
                Reply::Registered(profile),
            )
        }
        Op::FetchLink { id } => {
            if !valid_hex64(&id) {
                return Err(no("bad link id"));
            }
            (
                json!({"action":"fetch_link","link_id":id}),
                Reply::Link(facts.link_box.clone()),
            )
        }
        _ if !session.registered => return Err(no("register first")),
        op @ (Op::Lookup { .. } | Op::Profiles { .. } | Op::Devices) => {
            let selected = facts
                .reads
                .as_ref()
                .ok_or_else(|| internal("missing directory read facts"))?;
            (
                json!({"action":"read"}),
                reads::reply(&op, session, selected)?,
            )
        }
        Op::PublishKeyPackages {
            packages,
            last_resort,
        } => {
            let facts = facts
                .key_packages
                .as_ref()
                .ok_or_else(|| internal("missing key-package publication facts"))?;
            (
                key_packages::publications(session, &packages, last_resort.as_deref(), facts)?,
                Reply::Done,
            )
        }
        Op::ClaimKeyPackages { ids, operation_id } => {
            let facts = facts
                .key_packages
                .as_ref()
                .ok_or_else(|| internal("missing key-package claim facts"))?;
            let (plan, records) = key_packages::claim(&ids, operation_id.as_deref(), facts)?;
            (plan, Reply::KeyPackages(records))
        }
        Op::DeliverLink { id, sealed, device } => {
            if !valid_hex64(&id) || sealed.is_empty() || sealed.len() > MAX_LINK_BOX {
                return Err(no("bad link box"));
            }
            if let Some(device) = &device {
                if !enrollment_certificate_valid(&session.identity, &device.device, &device.cert) {
                    return Err(no("device certificate invalid"));
                }
                if facts
                    .target
                    .as_ref()
                    .is_some_and(|row| !enrollment_matches(row, &session.identity, &device.cert))
                {
                    return Err(no("device is revoked or belongs to another account"));
                }
            }
            if facts.link_box.is_some() {
                return Err(no("a link box is already there"));
            }
            (
                json!({"action":"deliver_link","link_id":id,"sealed":sealed,"device":device.map(|value| json!({"device":value.device,"cert":value.cert}))}),
                Reply::Done,
            )
        }
        Op::Unlink { device } => {
            if !facts
                .target
                .as_ref()
                .is_some_and(|row| row.identity == session.identity && !row.revoked)
            {
                return Err(no("not a linked device of yours"));
            }
            (json!({"action":"unlink","device":device}), Reply::Done)
        }
        _ => {
            return Err(BoundaryError::no(
                "unsupported_directory",
                "directory operation is outside the private adapter scope",
            ))
        }
    };
    plan["reply_base64"] = json!(STANDARD.encode(
        ServerFrame::Res {
            id,
            result: Ok(reply)
        }
        .encode()
    ));
    Ok(plan)
}

pub fn hello_json(bytes: &[u8], context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        nonce: String,
        relay: String,
        now_ms: String,
    }
    let context: Context = parse(context)?;
    if !valid_hex64(&context.nonce) || context.relay.is_empty() || context.relay.len() > 256 {
        return Err(internal("invalid challenge context"));
    }
    let hello = hello(bytes)?;
    let now_ms = context
        .now_ms
        .parse::<i64>()
        .map_err(|_| internal("invalid directory clock"))?;
    let server_time_ms = hello
        .capabilities
        .iter()
        .any(|cap| cap == roda_proto::KEY_PACKAGE_CLAIM_CAPABILITY)
        .then_some(now_ms);
    let challenge = ServerFrame::Challenge {
        nonce: context.nonce,
        relay: context.relay,
        protocol: hello.protocol.min(roda_proto::PROTOCOL_VERSION),
        capabilities: hello.capabilities.clone(),
        server_time_ms,
    };
    encoded(&json!({"hello":hello,"challenge_base64":STANDARD.encode(challenge.encode())}))
}
pub fn authenticate_json(bytes: &[u8], context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        hello: Hello,
        nonce: String,
        relay: String,
        facts: Facts,
    }
    let context: Context = parse(context)?;
    encoded(&authenticate(
        bytes,
        &context.hello,
        &context.nonce,
        &context.relay,
        &context.facts,
    )?)
}
pub fn request_json(bytes: &[u8], context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        session: Session,
        facts: Facts,
    }
    let context: Context = parse(context)?;
    encoded(&request(bytes, &context.session, &context.facts)?)
}
pub fn authorize_json(context: &str) -> Result<String, BoundaryError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        session: Session,
        facts: Facts,
    }
    let context: Context = parse(context)?;
    authorize(&context.session, &context.facts)?;
    encoded(
        &json!({"identity":context.session.identity,"device":context.session.device,"registered":context.facts.registered}),
    )
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: crate::portable::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value).unwrap_or_else(|_| "directory failed".into()),
        )
    }
    #[wasm_bindgen(js_name = directoryHello)]
    pub fn hello(bytes: &[u8], context: &str) -> Result<String, JsValue> {
        super::hello_json(bytes, context).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryAuthenticate)]
    pub fn authenticate(bytes: &[u8], context: &str) -> Result<String, JsValue> {
        super::authenticate_json(bytes, context).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryLookup)]
    pub fn lookup(bytes: &[u8], session: &str) -> Result<String, JsValue> {
        super::parse(session)
            .and_then(|session| super::lookup_for_session(bytes, &session))
            .and_then(|value| super::encoded(&value))
            .map_err(error)
    }
    #[wasm_bindgen(js_name = directoryKeyPackageReply)]
    pub fn key_package_reply(bytes: &[u8]) -> Result<String, JsValue> {
        super::key_packages::reply_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryReadReply)]
    pub fn read_reply(bytes: &[u8]) -> Result<String, JsValue> {
        super::reads::reply_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryLookupRate)]
    pub fn lookup_rate(context: &str) -> Result<String, JsValue> {
        super::reads::lookup_rate_json(context).map_err(error)
    }
    #[wasm_bindgen(js_name = directorySocketFrame)]
    pub fn socket_frame(bytes: &[u8]) -> Result<String, JsValue> {
        super::socket::incoming_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = directorySocketControl)]
    pub fn socket_control(context: &str) -> Result<String, JsValue> {
        super::socket::control_json(context).map_err(error)
    }
    #[wasm_bindgen(js_name = directorySocketDelivery)]
    pub fn socket_delivery(bytes: &[u8]) -> Result<String, JsValue> {
        super::socket::delivery_json(bytes).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryRequest)]
    pub fn request(bytes: &[u8], context: &str) -> Result<String, JsValue> {
        super::request_json(bytes, context).map_err(error)
    }
    #[wasm_bindgen(js_name = directoryAuthorize)]
    pub fn authorize(context: &str) -> Result<String, JsValue> {
        super::authorize_json(context).map_err(error)
    }
}
