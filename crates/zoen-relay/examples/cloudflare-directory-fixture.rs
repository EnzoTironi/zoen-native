//! Native PostgreSQL oracle for the private Cloudflare Person-directory adapter.
//!
//! Set ZOEN_TEST_PG to an existing test database. This example creates one fresh,
//! validated schema and gives every fixture connection ONLY that search_path. It
//! never migrates, truncates, or drops the database or an existing schema. The pool
//! closes and the owned schema is dropped on success and ordinary error paths,
//! before the single JSON stdout document is printed. Diagnostics use stderr.
//! Public fixed seed bytes are fixture data, never account credentials.

use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{Author, Signer};
use protocol::{auth_message, ClientFrame, DeviceCertificate, Op, Reply, ServerFrame};
use relay_admission::directory::{prepare_profile, Session};
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection, PgConnection, PgPool, Postgres, Transaction,
};
use std::{collections::BTreeMap, str::FromStr, time::Duration};
use types::{Identity, IdentityKind};
use zoen_relay::{db, key_package_claims};

#[path = "cloudflare-directory-fixture/reads.rs"]
mod reads_fixture;
#[path = "cloudflare-directory-fixture/sealed_profiles.rs"]
mod sealed_profiles_fixture;

const RELAY: &str = "zoen-cloudflare-private";
const NOW_MS: &str = "1700000000000";

// These are exactly the directory columns/constraints used by the native APIs
// below (migrations 0001, 0005, 0012, 0022, 0024). No agent tables are touched.
const TABLES: &str = r#"
CREATE TABLE identities (
    id TEXT PRIMARY KEY, handle TEXT UNIQUE, kind TEXT NOT NULL,
    owner TEXT REFERENCES identities(id), profile JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE devices (
    device TEXT PRIMARY KEY, identity TEXT NOT NULL REFERENCES identities(id),
    cert TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT now(), revoked_at TIMESTAMPTZ
);
CREATE TABLE link_boxes (
    id TEXT PRIMARY KEY, sealed BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE agreement_keys (
    identity TEXT PRIMARY KEY REFERENCES identities(id), public TEXT NOT NULL,
    device TEXT NOT NULL, sig TEXT NOT NULL, cert TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE profiles (
    identity TEXT PRIMARY KEY REFERENCES identities(id), version BIGINT NOT NULL,
    ciphertext BYTEA NOT NULL, device TEXT NOT NULL, sig TEXT NOT NULL, cert TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE key_packages (
    id BIGSERIAL PRIMARY KEY, identity TEXT NOT NULL REFERENCES identities(id),
    device TEXT NOT NULL, data BYTEA NOT NULL,
    last_resort BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX key_packages_by_device ON key_packages(identity, device, last_resort, id);
CREATE UNIQUE INDEX key_packages_one_last_resort ON key_packages(identity, device) WHERE last_resort;
CREATE TABLE key_package_publications (
    identity TEXT NOT NULL REFERENCES identities(id), device TEXT NOT NULL,
    package_hash BYTEA NOT NULL CHECK (octet_length(package_hash) = 32),
    expires_at BIGINT NOT NULL, PRIMARY KEY(identity, device, package_hash)
);
CREATE INDEX key_package_publications_expiry ON key_package_publications(expires_at);
CREATE TABLE key_package_claim_receipts (
    source_identity TEXT NOT NULL REFERENCES identities(id),
    source_device TEXT NOT NULL REFERENCES devices(device),
    operation_id TEXT NOT NULL CHECK(length(operation_id) = 26),
    targets TEXT[] NOT NULL,
    response BYTEA NOT NULL CHECK(octet_length(response) <= 8388608),
    expires_at_ms BIGINT NOT NULL,
    PRIMARY KEY(source_identity, source_device, operation_id)
);
CREATE INDEX key_package_claim_receipts_expiry ON key_package_claim_receipts(expires_at_ms);
"#;

fn author(root: u8, device: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[root; 32]),
        Signer::from_secret(&[device; 32]),
    )
}

fn certificate(who: &Author) -> Result<DeviceCertificate> {
    Ok(DeviceCertificate {
        device: who
            .device
            .clone()
            .context("fixture needs a device author")?,
        cert: who
            .cert
            .clone()
            .context("fixture needs a device certificate")?,
    })
}

fn profile(who: &Author, handle: &str) -> Identity {
    Identity {
        id: who.identity.clone(),
        kind: IdentityKind::Person,
        name: "This Person name must be cleared".into(),
        handle: handle.into(),
        tint_hex: "#415A77".into(),
        glyph: None,
        owner: None,
        bio: "This Person bio must be cleared".into(),
        owner_proof: None,
    }
}

// Explicit timestamp fixtures must bypass new_ulid's process-monotonic clock:
// testing a future operation must not move all later expiry/retry fixtures forward.
fn claim_operation(now_ms: i64) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut value = ((now_ms as u128) << 80)
        | u128::from(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut encoded = [0; 26];
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    for index in (0..26).rev() {
        encoded[index] = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(encoded.to_vec()).expect("ASCII fixture operation")
}

// Only public MLS packages leave this in-memory synthetic device. No device
// database, private key package material, or runtime account is written to disk.
fn mls_packages(who: &Author, count: usize, last_resort: bool) -> Result<Vec<Vec<u8>>> {
    let mut connection = rusqlite::Connection::open_in_memory()?;
    mls::migrate(&mut connection)?;
    let cert = certificate(who)?;
    let device = mls::Device::new(
        &connection,
        [77; 32],
        &who.identity,
        who.key.secret(),
        &cert.cert,
    )?;
    Ok(device.key_packages(count, last_resort)?)
}

async fn snapshot(pool: &PgPool) -> Result<Value> {
    let profiles: Vec<Value> = sqlx::query_scalar("SELECT profile FROM identities ORDER BY id")
        .fetch_all(pool)
        .await?;
    let devices: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT device, identity, cert, revoked_at IS NOT NULL FROM devices ORDER BY device",
    )
    .fetch_all(pool)
    .await?;
    let boxes: Vec<(String, Vec<u8>)> =
        sqlx::query_as("SELECT id, sealed FROM link_boxes ORDER BY id")
            .fetch_all(pool)
            .await?;
    let rows: Vec<(String, String, Vec<u8>, bool)> =
        sqlx::query_as("SELECT identity, device, data, last_resort FROM key_packages")
            .fetch_all(pool)
            .await?;
    let mut packages = rows
        .into_iter()
        .map(|(identity, device, data, last)| (identity, device, last, STANDARD.encode(data)))
        .collect::<Vec<_>>();
    packages.sort();
    let rows: Vec<(String, String, Vec<u8>, i64)> = sqlx::query_as(
        "SELECT identity, device, package_hash, expires_at FROM key_package_publications",
    )
    .fetch_all(pool)
    .await?;
    let mut publications = rows
        .into_iter()
        .map(|(identity, device, hash, expires)| (identity, device, hex::encode(hash), expires))
        .collect::<Vec<_>>();
    publications.sort();
    type ClaimRow = (String, String, String, Vec<String>, Vec<u8>, i64);
    let mut claims: Vec<ClaimRow> = sqlx::query_as(
        "SELECT source_identity, source_device, operation_id, targets, response, expires_at_ms
         FROM key_package_claim_receipts",
    )
    .fetch_all(pool)
    .await?;
    claims.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    Ok(json!({
        "identities": profiles,
        "devices": devices.into_iter().map(|(device, identity, cert, revoked)|
            json!({"device":device,"identity":identity,"cert":cert,"revoked":revoked}))
            .collect::<Vec<_>>(),
        "boxes": boxes.into_iter().map(|(id, sealed)|
            json!({"id":id,"sealed_base64":STANDARD.encode(sealed)})).collect::<Vec<_>>(),
        "packages": packages.into_iter().map(|(identity,device,last_resort,data)|
            json!({"identity":identity,"device":device,"data_base64":data,"last_resort":last_resort}))
            .collect::<Vec<_>>(),
        "publications": publications.into_iter().map(|(identity,device,hash,expires)|
            json!({"identity":identity,"device":device,"package_hash":hash,"expires_secs":expires.to_string()}))
            .collect::<Vec<_>>(),
        "claims": claims.into_iter().map(|(identity,device,operation,targets,response,expires)|
            json!({"source_identity":identity,"source_device":device,"operation_id":operation,
                "targets":targets,"response_base64":STANDARD.encode(response),"expires_ms":expires.to_string()}))
            .collect::<Vec<_>>()
    }))
}

// Follow session.rs's Hello/Auth checks and call its real directory queries. The
// challenge nonce is deterministic test input; this is not a public handshake host.
async fn native_login(
    pool: &PgPool,
    hello: &[u8],
    auth: &[u8],
    nonce: &str,
) -> Result<std::result::Result<Session, String>> {
    let ClientFrame::Hello {
        protocol,
        identity,
        device,
        cert,
        ..
    } = ClientFrame::decode(hello).map_err(|e| anyhow!(e.to_string()))?
    else {
        return Err(anyhow!("fixture did not encode Hello"));
    };
    ensure!(protocol == protocol::PROTOCOL_VERSION);
    if !event_log::verify_sig(&identity, &event_log::device_cert_message(&device), &cert) {
        return Ok(Err("device certificate invalid".into()));
    }
    let ClientFrame::Auth { sig } =
        ClientFrame::decode(auth).map_err(|e| anyhow!(e.to_string()))?
    else {
        return Err(anyhow!("fixture did not encode Auth"));
    };
    if !event_log::verify_sig(&device, &auth_message(nonce, RELAY), &sig) {
        return Ok(Err("login signature invalid".into()));
    }
    if db::device_revoked(pool, &device).await? {
        return Ok(Err("this device was unlinked".into()));
    }
    let registered = db::is_registered(pool, &identity).await?;
    if registered && !db::device_linked(pool, &identity, &device).await? {
        return Ok(Err("link this device first".into()));
    }
    // run() follows the handshake with an exact stored certificate match and a
    // fresh admission fence before Ready; a valid but differently encoded cert
    // must not skip those checks.
    if registered && !db::touch_device(pool, &identity, &device, &cert).await? {
        return Ok(Err("link this device first".into()));
    }
    let Some(authorization) = db::authorize_device(pool, &identity, &device, registered).await?
    else {
        return Ok(Err("this device was unlinked".into()));
    };
    authorization.rollback().await?;
    Ok(Ok(Session {
        identity,
        device,
        cert,
        registered,
    }))
}

fn valid_link_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

// Only the private-adapter operations are dispatched. The native SQL APIs
// perform every durable write; no expected state is supplied by the WASM planner.
async fn native_operation(
    pool: &PgPool,
    session: &Session,
    op: Op,
    authorization: Option<&mut Transaction<'_, Postgres>>,
) -> std::result::Result<Reply, String> {
    match op {
        Op::Register { profile } => {
            let profile = prepare_profile(profile, &session.identity)?;
            db::register(
                authorization.ok_or("registration authorization unavailable")?,
                &profile,
                &profile.handle,
                &session.device,
                &session.cert,
                !session.registered,
            )
            .await?;
            Ok(Reply::Registered(profile))
        }
        Op::FetchLink { id } => {
            if !valid_link_id(&id) {
                return Err("bad link id".into());
            }
            db::take_link_box(pool, &id)
                .await
                .map(Reply::Link)
                .map_err(|e| e.to_string())
        }
        _ if !session.registered => Err("register first".into()),
        Op::Lookup { handle, prefix } => {
            let handle = relay_admission::directory::reads::normalize_lookup(&handle);
            if handle.is_empty() {
                Ok(Reply::Profiles(Vec::new()))
            } else {
                db::lookup(pool, &handle, prefix)
                    .await
                    .map(Reply::Profiles)
                    .map_err(|e| e.to_string())
            }
        }
        Op::Profiles { ids } => {
            let ids = relay_admission::directory::reads::profile_ids(ids);
            db::profiles(pool, &ids)
                .await
                .map(Reply::Profiles)
                .map_err(|e| e.to_string())
        }
        Op::Devices => db::devices_of(pool, &session.identity)
            .await
            .map(Reply::Devices)
            .map_err(|e| e.to_string()),
        Op::PublishAgreementKey { public, signed } => {
            relay_admission::directory::sealed_profiles::validate_agreement(
                &session.identity,
                &session.device,
                &public,
                &signed,
            )?;
            db::put_agreement_key(pool, &session.identity, &public, &signed)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Reply::Done)
        }
        Op::AgreementKeys { ids } => {
            db::agreement_keys(pool, &relay_admission::directory::reads::profile_ids(ids))
                .await
                .map(Reply::AgreementKeys)
                .map_err(|e| e.to_string())
        }
        Op::PutProfile { profile } => {
            relay_admission::directory::sealed_profiles::validate_profile(
                &session.identity,
                &session.device,
                &profile,
            )?;
            if !db::put_profile(pool, &profile)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("stale profile version".into());
            }
            Ok(Reply::Done)
        }
        Op::GetProfiles { ids } => {
            db::sealed_profiles(pool, &relay_admission::directory::reads::profile_ids(ids))
                .await
                .map(Reply::SealedProfiles)
                .map_err(|e| e.to_string())
        }
        Op::DeliverLink { id, sealed, device } => {
            if !valid_link_id(&id) || sealed.is_empty() || sealed.len() > db::MAX_LINK_BOX {
                return Err("bad link box".into());
            }
            let authorization = authorization.ok_or("link authorization unavailable")?;
            if let Some(device) = device {
                db::enroll_device(
                    authorization,
                    &session.identity,
                    &device.device,
                    &device.cert,
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            if !db::put_link_box(authorization, &id, &sealed)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("a link box is already there".into());
            }
            Ok(Reply::Done)
        }
        Op::Unlink { device } => {
            if !db::revoke_device(
                authorization.ok_or("unlink authorization unavailable")?,
                &session.identity,
                &device,
            )
            .await
            .map_err(|e| e.to_string())?
            {
                return Err("not a linked device of yours".into());
            }
            Ok(Reply::Done)
        }
        Op::PublishKeyPackages {
            packages,
            last_resort,
        } => {
            let publications = relay_admission::directory::key_packages::validate_publications(
                &session.identity,
                &session.device,
                &packages,
                last_resort.as_deref(),
            )
            .map_err(|e| e.to_string())?;
            if !db::put_key_packages(
                pool,
                &session.identity,
                &session.device,
                &publications,
                last_resort.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?
            {
                return Err("too many key packages stored".into());
            }
            Ok(Reply::Done)
        }
        Op::ClaimKeyPackages { ids, operation_id } => {
            let records = if let Some(operation) = operation_id {
                key_package_claims::claim(
                    pool,
                    &session.identity,
                    &session.device,
                    &operation,
                    &ids,
                )
                .await
                .map_err(|e| e.to_string())?
            } else {
                let ids = ids.into_iter().take(50).collect::<Vec<_>>();
                db::claim_key_packages(pool, &ids)
                    .await
                    .map_err(|e| e.to_string())?
            };
            Ok(Reply::KeyPackages(records))
        }
        _ => Err("fixture operation outside private Directory scope".into()),
    }
}

async fn native_request(
    pool: &PgPool,
    session: &Session,
    op: Op,
) -> Result<std::result::Result<Reply, String>> {
    let Some(authorization) =
        db::authorize_device(pool, &session.identity, &session.device, session.registered).await?
    else {
        // Native session.rs closes a freshly refused socket without a Res frame;
        // the private adapter reports this equivalent refusal string instead.
        return Ok(Err("this device was unlinked".into()));
    };
    let directory_change = matches!(
        &op,
        Op::Register { .. } | Op::DeliverLink { .. } | Op::Unlink { .. }
    );
    let mut authorization = if directory_change {
        Some(authorization)
    } else {
        authorization.commit().await?;
        None
    };
    let result = native_operation(pool, session, op, authorization.as_mut()).await;
    if let Some(authorization) = authorization {
        if result.is_ok() {
            authorization.commit().await?;
        } else {
            authorization.rollback().await?;
        }
    }
    Ok(result)
}

struct Fixture<'a> {
    pool: &'a PgPool,
    sessions: BTreeMap<String, Session>,
    steps: Vec<Value>,
    now_ms: String,
}

impl<'a> Fixture<'a> {
    async fn login(
        &mut self,
        label: &str,
        actor: &str,
        who: &Author,
        rejected: Option<&str>,
    ) -> Result<()> {
        let device = certificate(who)?;
        let hello = ClientFrame::Hello {
            protocol: protocol::PROTOCOL_VERSION,
            capabilities: Vec::new(),
            identity: who.identity.clone(),
            device: device.device,
            cert: device.cert,
        }
        .encode();
        let nonce = format!("{:064x}", self.steps.len() + 1);
        let auth = ClientFrame::Auth {
            sig: who.key.sign(&auth_message(&nonce, RELAY)),
        }
        .encode();
        let before = snapshot(self.pool).await?;
        let expected = match native_login(self.pool, &hello, &auth, &nonce).await? {
            Ok(session) => {
                ensure!(
                    rejected.is_none(),
                    "{label}: native login unexpectedly succeeded"
                );
                let expected = json!({"kind":"session","identity":session.identity,
                    "device":session.device,"cert":session.cert,"registered":session.registered});
                self.sessions.insert(actor.into(), session);
                expected
            }
            Err(reason) => {
                ensure!(
                    rejected == Some(reason.as_str()),
                    "{label}: unexpected native login refusal: {reason}"
                );
                json!({"kind":"rejected","reason":reason})
            }
        };
        let after = snapshot(self.pool).await?;
        ensure!(
            before == after,
            "{label}: native login mutated directory data"
        );
        let challenge = ServerFrame::Challenge {
            nonce: nonce.clone(),
            relay: RELAY.into(),
            protocol: protocol::PROTOCOL_VERSION,
            capabilities: Vec::new(),
            server_time_ms: None,
        }
        .encode();
        let ready = self
            .sessions
            .get(actor)
            .filter(|_| rejected.is_none())
            .map(|session| {
                STANDARD.encode(
                    ServerFrame::Ready {
                        identity: session.identity.clone(),
                        registered: session.registered,
                    }
                    .encode(),
                )
            });

        self.steps
            .push(json!({"op":"login","label":label,"actor":actor,
            "hello_base64":STANDARD.encode(hello),"nonce":nonce,"auth_base64":STANDARD.encode(auth),
            "challenge_base64":STANDARD.encode(challenge),"ready_base64":ready,
            "now_ms":self.now_ms,"expected":expected,"state_after":after}));
        Ok(())
    }

    async fn frame(
        &mut self,
        label: &str,
        actor: &str,
        frame: &ClientFrame,
        rejected: Option<&str>,
    ) -> Result<()> {
        let bytes = frame.encode();
        let ClientFrame::Req { id, op } =
            ClientFrame::decode(&bytes).map_err(|e| anyhow!(e.to_string()))?
        else {
            return Err(anyhow!("fixture did not encode Req"));
        };
        let registering = matches!(&op, Op::Register { .. });
        let session = self
            .sessions
            .get(actor)
            .context("fixture actor not logged in")?
            .clone();
        let before = snapshot(self.pool).await?;
        let expected = match native_request(self.pool, &session, op).await? {
            Ok(reply) => {
                ensure!(
                    rejected.is_none(),
                    "{label}: native request unexpectedly succeeded"
                );
                if registering {
                    self.sessions
                        .get_mut(actor)
                        .context("fixture actor disappeared")?
                        .registered = true;
                }
                json!({"kind":"reply","reply_base64":STANDARD.encode(ServerFrame::Res {id,result:Ok(reply)}.encode())})
            }
            Err(reason) => {
                ensure!(
                    rejected == Some(reason.as_str()),
                    "{label}: unexpected native request refusal: {reason}"
                );
                json!({"kind":"rejected","reason":reason})
            }
        };
        let after = snapshot(self.pool).await?;
        if rejected.is_some() {
            ensure!(
                before == after,
                "{label}: refused native transaction left a partial write"
            );
        }
        let refused_reply = expected
            .get("reason")
            .and_then(Value::as_str)
            .map(|reason| {
                STANDARD.encode(
                    ServerFrame::Res {
                        id,
                        result: Err(reason.into()),
                    }
                    .encode(),
                )
            });
        self.steps.push(json!({"op":"request","label":label,"actor":actor,
            "frame_base64":STANDARD.encode(bytes),"now_ms":self.now_ms,"expected":expected,"state_after":after,
            "refused_reply_base64":refused_reply}));
        Ok(())
    }

    async fn request(
        &mut self,
        label: &str,
        actor: &str,
        op: Op,
        rejected: Option<&str>,
    ) -> Result<()> {
        let frame = ClientFrame::Req {
            id: self.steps.len() as u64 + 1000,
            op,
        };
        self.frame(label, actor, &frame, rejected).await
    }
}

async fn timeline(pool: &PgPool) -> Result<Value> {
    sqlx::raw_sql(TABLES).execute(pool).await?;
    ensure!(
        snapshot(pool).await?
            == json!({"identities":[],"devices":[],"boxes":[],
        "packages":[],"publications":[],"claims":[]})
    );
    let mut f = Fixture {
        pool,
        sessions: BTreeMap::new(),
        steps: Vec::new(),
        now_ms: NOW_MS.into(),
    };
    let a = author(11, 21);
    let b = author(12, 22);
    let pending = author(31, 31);
    let linked = author(11, 31);
    let new_target = author(11, 32);
    let foreign_target = author(12, 31);
    let a_profile = profile(&a, "  @ADA_FIXTURE  ");
    let link_id = "a1".repeat(32);
    let conflict_id = "b2".repeat(32);

    f.login("login-first", "a", &a, None).await?;
    f.login("login-preopened-same-device", "a-stale", &a, None)
        .await?;
    f.request(
        "register-first",
        "a",
        Op::Register {
            profile: a_profile.clone(),
        },
        None,
    )
    .await?;
    f.request(
        "register-stale",
        "a-stale",
        Op::Register {
            profile: a_profile.clone(),
        },
        Some("account already registered; link this device first"),
    )
    .await?;
    f.login("login-registered-active-device", "a-fresh", &a, None)
        .await?;
    let uppercase_cert = Author::certified(
        a.identity.clone(),
        a.key.clone(),
        certificate(&a)?.cert.to_ascii_uppercase(),
    );
    f.login(
        "login-known-device-uppercase-certificate",
        "uppercase-cert",
        &uppercase_cert,
        Some("link this device first"),
    )
    .await?;
    f.request(
        "register-active-profile",
        "a",
        Op::Register {
            profile: a_profile.clone(),
        },
        None,
    )
    .await?;
    f.login(
        "login-root-certified-unenrolled",
        "unlinked",
        &author(11, 23),
        Some("link this device first"),
    )
    .await?;
    f.login("login-second-account", "b", &b, None).await?;
    f.request(
        "register-handle-conflict",
        "b",
        Op::Register {
            profile: profile(&b, "ada_fixture"),
        },
        Some("handle_taken"),
    )
    .await?;
    f.request(
        "register-second-account",
        "b",
        Op::Register {
            profile: profile(&b, "bea_fixture"),
        },
        None,
    )
    .await?;
    f.login("login-pending-self-key", "pending", &pending, None)
        .await?;
    f.request(
        "fetch-pending-empty",
        "pending",
        Op::FetchLink {
            id: link_id.clone(),
        },
        None,
    )
    .await?;
    f.request(
        "deliver-provisional-refused",
        "pending",
        Op::DeliverLink {
            id: conflict_id.clone(),
            sealed: vec![1],
            device: None,
        },
        Some("register first"),
    )
    .await?;

    let delivery = ClientFrame::Req {
        id: 100,
        op: Op::DeliverLink {
            id: link_id.clone(),
            sealed: vec![7, 9, 11],
            device: Some(certificate(&linked)?),
        },
    };
    f.frame("deliver-first", "a", &delivery, None).await?;
    f.login(
        "login-pending-self-key-after-assignment",
        "pending-fresh",
        &pending,
        None,
    )
    .await?;
    f.request(
        "fetch-first",
        "pending",
        Op::FetchLink {
            id: link_id.clone(),
        },
        None,
    )
    .await?;
    f.request(
        "fetch-one-time-empty",
        "pending",
        Op::FetchLink {
            id: link_id.clone(),
        },
        None,
    )
    .await?;
    // authorize_device admits this still-provisional self-key session. register
    // inserts its identity first, then enrollment conflicts with A's device row;
    // rollback must remove that identity again, not leave a partial account.
    f.request(
        "register-first-enrollment-rollback",
        "pending",
        Op::Register {
            profile: profile(&pending, "pending_fixture"),
        },
        Some("device is revoked or belongs to another account"),
    )
    .await?;
    // Reuse the *identical* canonical request after the one-time box was taken.
    f.frame("deliver-active-exact-retry", "a", &delivery, None)
        .await?;
    f.frame(
        "deliver-occupied-exact-retry",
        "a",
        &delivery,
        Some("a link box is already there"),
    )
    .await?;
    // Enrollment succeeds before the occupied box is detected. Both writes must
    // roll back together, leaving new_target absent from the next snapshot.
    f.request(
        "deliver-occupied-box-enrollment-rollback",
        "a",
        Op::DeliverLink {
            id: link_id.clone(),
            sealed: vec![13],
            device: Some(certificate(&new_target)?),
        },
        Some("a link box is already there"),
    )
    .await?;
    f.request(
        "deliver-global-device-owner-conflict",
        "b",
        Op::DeliverLink {
            id: conflict_id,
            sealed: vec![17],
            device: Some(certificate(&foreign_target)?),
        },
        Some("device is revoked or belongs to another account"),
    )
    .await?;
    f.login("login-sponsored-target", "linked", &linked, None)
        .await?;
    f.request(
        "fetch-sponsored-target",
        "linked",
        Op::FetchLink {
            id: link_id.clone(),
        },
        None,
    )
    .await?;
    f.request(
        "unlink-sponsored-target",
        "a",
        Op::Unlink {
            device: certificate(&linked)?.device,
        },
        None,
    )
    .await?;
    f.request(
        "revoked-session-fresh-request",
        "linked",
        Op::FetchLink {
            id: link_id.clone(),
        },
        Some("this device was unlinked"),
    )
    .await?;
    f.login(
        "revoked-device-fresh-login",
        "revoked",
        &linked,
        Some("this device was unlinked"),
    )
    .await?;
    f.frame(
        "deliver-cannot-revive-revoked-device",
        "a",
        &delivery,
        Some("device is revoked or belongs to another account"),
    )
    .await?;
    f.request(
        "unlink-foreign-device",
        "a",
        Op::Unlink {
            device: certificate(&b)?.device,
        },
        Some("not a linked device of yours"),
    )
    .await?;
    f.request(
        "unlink-self",
        "b",
        Op::Unlink {
            device: certificate(&b)?.device,
        },
        None,
    )
    .await?;
    f.request(
        "self-revoked-session-fresh-register",
        "b",
        Op::Register {
            profile: profile(&b, "bea_fixture"),
        },
        Some("this device was unlinked"),
    )
    .await?;
    f.login(
        "self-revoked-device-fresh-login",
        "b-revoked",
        &b,
        Some("this device was unlinked"),
    )
    .await?;

    key_package_timeline(&mut f, &a).await?;
    let steps = std::mem::take(&mut f.steps);
    reads_fixture::timeline(&mut f).await?;
    let read_steps = std::mem::take(&mut f.steps);
    let crypto_proof = sealed_profiles_fixture::timeline(&mut f).await?;
    Ok(
        json!({"schema":1,"cell":"isolated_directory_fixture","initial_empty":true,"steps":steps,
            "reads":{"steps":read_steps},"sealed_profiles":{"steps":f.steps,"crypto_proof":crypto_proof},"transport":socket_fixture()}),
    )
}

// Native codec oracle for the new PRIVATE socket transport. It adds no request
// steps or persistence writes; normal native operation results remain above.
fn socket_fixture() -> Value {
    let large_id = 9_007_199_254_740_993;
    let encode = |frame: ServerFrame| STANDARD.encode(frame.encode());
    json!({
        "ping_base64":STANDARD.encode(ClientFrame::Ping.encode()),
        "pong_base64":encode(ServerFrame::Pong { server_time_ms: None }),
        "malformed_auth_base64":STANDARD.encode(ClientFrame::Ping.encode()),
        "malformed_auth_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Other,"expected auth")),
        "large_request_base64":STANDARD.encode(ClientFrame::Req { id:large_id, op:Op::FetchLink { id:"0".repeat(64) } }.encode()),
        "large_reply_base64":encode(ServerFrame::Res { id:large_id, result:Ok(Reply::Link(None)) }),
        "unsupported_request_base64":STANDARD.encode(ClientFrame::Req { id:large_id, op:Op::RegisterPush { platform:"apns".into(), token:String::new() } }.encode()),
        "unsupported_reply_base64":encode(ServerFrame::Res { id:large_id, result:Err("directory operation is outside the private adapter scope".into()) }),
        "unsupported_space_base64":STANDARD.encode(ClientFrame::Sync { cursors:Vec::new(), all:true }.encode()),
        "unsupported_space_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Other,"private directory socket does not support Space operations")),
        "expired_hello_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Other,"expected hello")),
        "expired_auth_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Other,"expected auth")),
        "already_logged_in_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Other,"already logged in")),
        "certificate_mismatch_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Unauthorized,"link this device first")),
        "storage_error_base64":encode(ServerFrame::error(protocol::ErrorCode::Unavailable,"database unavailable"))
    })
}

async fn key_package_timeline(f: &mut Fixture<'_>, a: &Author) -> Result<()> {
    let now = key_package_claims::clock_ms(f.pool).await?;
    f.now_ms = now.to_string();
    // The private host's bounded sessions from the 2023 prefix have expired.
    f.login("kp-login-active-source", "a-kp", a, None).await?;
    let c = author(13, 33);
    let d = author(14, 34);
    f.login("kp-login-target-c", "c", &c, None).await?;
    f.request(
        "kp-register-target-c",
        "c",
        Op::Register {
            profile: profile(&c, "kp_c_fixture"),
        },
        None,
    )
    .await?;
    f.login("kp-login-target-d", "d", &d, None).await?;
    f.request(
        "kp-register-target-d",
        "d",
        Op::Register {
            profile: profile(&d, "kp_d_fixture"),
        },
        None,
    )
    .await?;
    let cp = mls_packages(&c, 4, true)?;
    let dp = mls_packages(&d, 2, true)?;
    let replacement = mls_packages(&c, 0, true)?.remove(0);
    f.request(
        "kp-publish-first",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[0].clone(), cp[0].clone()],
            last_resort: Some(cp[4].clone()),
        },
        None,
    )
    .await?;
    // Separate successful publications give an authoritative native FIFO order;
    // the test projection never compares native BIGSERIAL/SQLite rowid gaps.
    f.request(
        "kp-publish-second",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[1].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-other-target",
        "d",
        Op::PublishKeyPackages {
            packages: vec![dp[0].clone()],
            last_resort: Some(dp[2].clone()),
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-wrong-device",
        "c",
        Op::PublishKeyPackages {
            packages: vec![dp[1].clone()],
            last_resort: None,
        },
        Some("key package is for another device"),
    )
    .await?;
    f.request(
        "kp-publish-malformed",
        "c",
        Op::PublishKeyPackages {
            packages: vec![vec![0]],
            last_resort: None,
        },
        Some("not a valid key package"),
    )
    .await?;
    f.request(
        "kp-publish-oversized",
        "c",
        Op::PublishKeyPackages {
            packages: vec![vec![0; 4097]],
            last_resort: None,
        },
        Some("key package too large"),
    )
    .await?;
    f.request(
        "kp-publish-over100",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[2].clone(); 101],
            last_resort: None,
        },
        Some("at most 100 key packages at a time"),
    )
    .await?;

    let operation = claim_operation(now);
    let first_claim = ClientFrame::Req {
        id: 2000,
        op: Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(operation.clone()),
        },
    };
    f.frame("kp-claim-first", "a-kp", &first_claim, None)
        .await?;
    f.frame("kp-claim-replay", "a-kp", &first_claim, None)
        .await?;
    f.request(
        "kp-publish-consumed-retry",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[0].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-claim-changed-targets",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![d.identity.clone()],
            operation_id: Some(operation.clone()),
        },
        Some("key-package claim operation reused with different targets"),
    )
    .await?;
    f.request(
        "kp-claim-noncanonical-target",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.to_ascii_uppercase()],
            operation_id: Some(claim_operation(now)),
        },
        Some("claim targets must be canonical identity keys"),
    )
    .await?;
    f.request(
        "kp-claim-empty-targets",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: Vec::new(),
            operation_id: Some(claim_operation(now)),
        },
        Some("claim one to fifty identities"),
    )
    .await?;
    f.request(
        "kp-claim-malformed-operation",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some("invalid-operation".into()),
        },
        Some("invalid key-package claim operation"),
    )
    .await?;
    f.request(
        "kp-claim-future-operation",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(now + 60_000)),
        },
        Some("key-package claim operation is in the future"),
    )
    .await?;
    f.request(
        "kp-claim-expired-operation",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(
                now - protocol::KEY_PACKAGE_CLAIM_TTL_MS - 1,
            )),
        },
        Some(protocol::KEY_PACKAGE_CLAIM_EXPIRED),
    )
    .await?;
    f.request(
        "kp-claim-fifo-second",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(now)),
        },
        None,
    )
    .await?;
    f.request(
        "kp-last-resort-replacement",
        "c",
        Op::PublishKeyPackages {
            packages: Vec::new(),
            last_resort: Some(replacement),
        },
        None,
    )
    .await?;
    f.request(
        "kp-claim-fallback",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(now)),
        },
        None,
    )
    .await?;
    f.request(
        "kp-claim-fallback-retained",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(now)),
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-third",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[2].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-other-target-second",
        "d",
        Op::PublishKeyPackages {
            packages: vec![dp[1].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    // SQL does not promise inter-device result order. Keep the actual native
    // bytes, and mark only this fresh multi-target comparison as set-valued.
    f.request(
        "kp-claim-two-targets",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![d.identity.clone(), c.identity.clone(), c.identity.clone()],
            operation_id: Some(claim_operation(now)),
        },
        None,
    )
    .await?;
    f.steps.last_mut().context("multi-target step missing")?["unordered_reply"] = json!(true);
    f.request(
        "kp-publish-multi-consumed-retry",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[2].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    let mut first_fifty = vec![d.identity.clone(); 50];
    first_fifty.push(c.identity.clone());
    f.request(
        "kp-legacy-first50-duplicates",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: first_fifty,
            operation_id: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-legacy-empty",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: Vec::new(),
            operation_id: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-before-target-revoke",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[3].clone()],
            last_resort: None,
        },
        None,
    )
    .await?;
    f.request(
        "kp-unlink-target",
        "c",
        Op::Unlink {
            device: certificate(&c)?.device,
        },
        None,
    )
    .await?;
    f.frame("kp-claim-target-revoked-replay", "a-kp", &first_claim, None)
        .await?;
    f.request(
        "kp-claim-target-revoked-fresh",
        "a-kp",
        Op::ClaimKeyPackages {
            ids: vec![c.identity.clone()],
            operation_id: Some(claim_operation(now)),
        },
        None,
    )
    .await?;
    f.request(
        "kp-publish-revoked-target",
        "c",
        Op::PublishKeyPackages {
            packages: vec![cp[3].clone()],
            last_resort: None,
        },
        Some("this device was unlinked"),
    )
    .await?;
    f.request(
        "kp-unlink-caller",
        "a-kp",
        Op::Unlink {
            device: certificate(a)?.device,
        },
        None,
    )
    .await?;
    f.frame(
        "kp-claim-caller-revoked",
        "a-kp",
        &first_claim,
        Some("this device was unlinked"),
    )
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let url =
        std::env::var("ZOEN_TEST_PG").context("set ZOEN_TEST_PG to an existing test database")?;
    let options = PgConnectOptions::from_str(&url)?;
    let mut admin = PgConnection::connect_with(&options).await?;
    let schema = format!("cf_directory_{}", types::new_id("t"));
    ensure!(
        schema.len() <= 63
            && schema
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
    );
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA \"{schema}\"")))
        .execute(&mut admin)
        .await?;
    eprintln!("Directory oracle owns fresh disposable schema {schema}");
    let result = async {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options.options([("search_path", schema.as_str())]))
            .await?;
        let result = async {
            let current: String = sqlx::query_scalar("SELECT current_schema()")
                .fetch_one(&pool)
                .await?;
            let search_path: String = sqlx::query_scalar("SHOW search_path")
                .fetch_one(&pool)
                .await?;
            ensure!(
                current == schema && search_path == schema,
                "fixture search_path did not select its owned schema"
            );
            timeline(&pool).await
        }
        .await;
        pool.close().await;
        result
    }
    .await;
    // No IF EXISTS: disappearance of our namespace is an error, never permission
    // to clean up a different schema. CASCADE is confined to the fresh schema.
    let cleanup = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA \"{schema}\" CASCADE"
    )))
    .execute(&mut admin)
    .await;
    match (result, cleanup) {
        (Ok(value), Ok(_)) => println!("{}", serde_json::to_string(&value)?),
        (Err(error), Ok(_)) => return Err(error),
        (Ok(_), Err(error)) => return Err(error).context("owned schema cleanup failed"),
        (Err(error), Err(cleanup)) => {
            return Err(error).context(format!("owned schema cleanup also failed: {cleanup}"));
        }
    }
    Ok(())
}
