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
use roda_log::{Author, Signer};
use roda_proto::{auth_message, ClientFrame, DeviceCertificate, Op, Reply, ServerFrame};
use roda_relay_admission::directory::{prepare_profile, Session};
use roda_types::{Identity, IdentityKind};
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Connection, PgConnection, PgPool, Postgres, Transaction,
};
use std::{collections::BTreeMap, str::FromStr, time::Duration};
use zoen_relay::db;

const RELAY: &str = "zoen-cloudflare-private";
const NOW_MS: &str = "1700000000000";

// These are exactly the directory columns/constraints used by the native APIs
// below (migrations 0001, 0005, 0012). Person-only inputs never touch agent tables.
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
CREATE TABLE key_packages (
    id BIGSERIAL PRIMARY KEY, identity TEXT NOT NULL REFERENCES identities(id),
    device TEXT NOT NULL, data BYTEA NOT NULL,
    last_resort BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX key_packages_by_device ON key_packages(identity, device, last_resort, id);
CREATE UNIQUE INDEX key_packages_one_last_resort ON key_packages(identity, device) WHERE last_resort;
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
    Ok(json!({
        "identities": profiles,
        "devices": devices.into_iter().map(|(device, identity, cert, revoked)|
            json!({"device":device,"identity":identity,"cert":cert,"revoked":revoked}))
            .collect::<Vec<_>>(),
        "boxes": boxes.into_iter().map(|(id, sealed)|
            json!({"id":id,"sealed_base64":STANDARD.encode(sealed)})).collect::<Vec<_>>()
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
    ensure!(protocol == roda_proto::PROTOCOL_VERSION);
    if !roda_log::verify_sig(&identity, &roda_log::device_cert_message(&device), &cert) {
        return Ok(Err("device certificate invalid".into()));
    }
    let ClientFrame::Auth { sig } =
        ClientFrame::decode(auth).map_err(|e| anyhow!(e.to_string()))?
    else {
        return Err(anyhow!("fixture did not encode Auth"));
    };
    if !roda_log::verify_sig(&device, &auth_message(nonce, RELAY), &sig) {
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

// Only the four private-adapter operations are dispatched. The native SQL APIs
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
            protocol: roda_proto::PROTOCOL_VERSION,
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
        self.steps
            .push(json!({"op":"login","label":label,"actor":actor,
            "hello_base64":STANDARD.encode(hello),"nonce":nonce,"auth_base64":STANDARD.encode(auth),
            "now_ms":NOW_MS,"expected":expected,"state_after":after}));
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
        self.steps.push(json!({"op":"request","label":label,"actor":actor,
            "frame_base64":STANDARD.encode(bytes),"now_ms":NOW_MS,"expected":expected,"state_after":after}));
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
    ensure!(snapshot(pool).await? == json!({"identities":[],"devices":[],"boxes":[]}));
    let mut f = Fixture {
        pool,
        sessions: BTreeMap::new(),
        steps: Vec::new(),
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

    Ok(json!({"schema":1,"cell":"isolated_directory_fixture","initial_empty":true,"steps":f.steps}))
}

#[tokio::main]
async fn main() -> Result<()> {
    let url =
        std::env::var("ZOEN_TEST_PG").context("set ZOEN_TEST_PG to an existing test database")?;
    let options = PgConnectOptions::from_str(&url)?;
    let mut admin = PgConnection::connect_with(&options).await?;
    let schema = format!("cf_directory_{}", roda_types::new_id("t"));
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
