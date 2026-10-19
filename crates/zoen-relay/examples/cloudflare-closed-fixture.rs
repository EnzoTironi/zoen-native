//! Native FoundationDB oracle for the private Cloudflare closed-group projection.
//!
//! With an existing FDB cluster configured, run this example and feed its one JSON
//! stdout document to the Worker differential harness. All keys belong to a fresh
//! `new_id("t")` cell; cleanup runs on both success and ordinary error paths before
//! any fixture is printed. Public seed bytes below are test data, never credentials.

use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{Author, Signer};
use foundationdb::{
    tuple::{pack, unpack, Subspace},
    Database,
};
use protocol::{ClientFrame, Envelope, Sequenced};
use relay_admission::{
    code_hash,
    invite::invite_limits,
    portable::{verify_publish, SessionContext},
};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use types::{EventBody, Privacy, Role, Seen, SpaceKind};
use zoen_relay::log::{fdb::FdbLog, LogStore, Sequencing};

#[path = "cloudflare-closed-fixture/mls.rs"]
mod mls;

// This is the native FDB invite tuple, read back instead of predicting storage.
type StoredInvite = (String, String, String, i64, i64, i64);

fn now_ms() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

fn author(identity: u8, device: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[identity; 32]),
        Signer::from_secret(&[device; 32]),
    )
}

fn message(number: u32, text: &str) -> EventBody {
    EventBody::MessagePosted {
        message: format!("m_closed_fixture_{number}"),
        text: text.into(),
        attaches: None,
        reply: None,
        forwarded: false,
    }
}

fn session(who: &Author) -> Result<SessionContext> {
    Ok(SessionContext {
        authenticated_identity: who.identity.clone(),
        authenticated_device: who
            .device
            .clone()
            .context("fixture needs a device author")?,
    })
}

fn negative_case(
    label: &str,
    who: &Author,
    env: Envelope,
    code: &str,
    permanent: bool,
    reason: &str,
) -> Result<Value> {
    let space_id = env.space().to_string();
    let frame = ClientFrame::Publish { env }.encode();
    let session = session(who)?;
    let expected_error = match verify_publish(&frame, &session) {
        Err(error) => {
            ensure!(
                error.code == code && error.permanent == permanent,
                "{label}: unexpected verification error"
            );
            serde_json::to_value(error)?
        }
        Ok(_) => {
            ensure!(
                code != "bad_signature",
                "{label}: signature should be invalid"
            );
            json!({ "code": code, "reason": reason, "permanent": permanent })
        }
    };
    let step = json!({
        "op": "publish", "label": label, "frame_base64": STANDARD.encode(frame),
        "session": session, "target_registered": null, "target_identity": null,
        "now_ms": now_ms()?.to_string(), "expected_projection_error": code,
        "expected_projection_permanent": permanent, "expected_error": expected_error,
    });
    Ok(
        json!({ "space_id": space_id, "steps": [step], "expected_projection_error": code,
            "expected_projection_permanent": permanent, "expected_error": expected_error }),
    )
}

enum Expected {
    New,
    Duplicate,
    Rejected(&'static str),
}

struct Journey<'a> {
    log: &'a FdbLog,
    db: &'a Database,
    cell: &'a str,
    space: String,
    head: Option<Seen>,
    steps: Vec<Value>,
}

impl Journey<'_> {
    fn signed(&self, who: &Author, number: u32, body: EventBody) -> Envelope {
        // Decimal-only, zero-padded CIDs are valid ULIDs and reproducible per step.
        Envelope::plain(&who.sign_event(
            &self.space,
            &format!("{number:026}"),
            1_700_000_000_000 + i64::from(number),
            self.head.clone(),
            body,
        ))
    }

    async fn publish(
        &mut self,
        label: &str,
        who: &Author,
        env: Envelope,
        target_registered: Option<bool>,
        wanted: Expected,
    ) -> Result<Option<Sequenced>> {
        let frame = ClientFrame::Publish { env: env.clone() }.encode();
        let session = session(who)?;
        let target_identity = match env.body() {
            Some(
                EventBody::MemberAdded { identity, .. } | EventBody::MemberRemoved { identity },
            ) => Some(identity),
            _ => None,
        };
        // FdbLog trusts its authenticated ingress. Verify that this fixture supplies
        // a genuinely signed frame bound to exactly the supplied device session.
        verify_publish(&frame, &session).map_err(|e| anyhow!("{label}: {}", e.reason))?;
        let observed_now = now_ms()?;
        let outcome = self
            .log
            .append(&env, target_registered.unwrap_or(true))
            .await;
        let (expected, event) = match outcome {
            Ok(result) => {
                let (kind, ev, mut audience, joined) = match result {
                    Sequencing::New {
                        ev,
                        audience,
                        joined,
                    } => ("new", ev, audience, joined),
                    Sequencing::Duplicate {
                        ev,
                        audience,
                        joined,
                    } => ("duplicate", ev, audience, joined),
                };
                ensure!(
                    matches!(
                        (&wanted, kind),
                        (Expected::New, "new") | (Expected::Duplicate, "duplicate")
                    ),
                    "{label}: unexpected native {kind} receipt"
                );
                audience.sort();
                if kind == "new" {
                    self.head = Some(Seen {
                        seq: ev.seq,
                        hash: ev.hash.clone(),
                    });
                }
                let expected = json!({
                    "kind": kind,
                    "event": {
                        "seq": ev.seq.to_string(), "prev": ev.prev, "hash": ev.hash,
                        "bytes_base64": STANDARD.encode(ev.encode()),
                    },
                    "audience": audience, "joined": joined,
                });
                (expected, Some(ev))
            }
            Err(rejection) => {
                let Expected::Rejected(reason) = wanted else {
                    return Err(anyhow!(
                        "{label}: unexpected rejection: {}",
                        rejection.reason
                    ));
                };
                ensure!(rejection.reason == reason, "{label}: {}", rejection.reason);
                ensure!(rejection.permanent, "{label}: rejection must be permanent");
                (
                    json!({ "kind": "rejected", "reason": rejection.reason, "permanent": rejection.permanent }),
                    None,
                )
            }
        };
        let tracking = mls::tracking(self).await?;
        let clock = tracking
            .hold_clock()
            .unwrap_or(observed_now)
            .max(observed_now);
        let mut roles = self.log.members(&self.space).await?;
        roles.sort_by(|a, b| a.0.cmp(&b.0));
        self.steps.push(json!({
            "op": "publish", "label": label, "frame_base64": STANDARD.encode(frame),
            "session": session, "target_registered": target_registered,
            "target_identity": target_identity,
            "now_ms": clock.to_string(), "expected": expected, "roles_after": roles,
            "tracking_after": tracking.value(),
            "history_after": if label.contains("checkpoint") || label.contains("pruning") { Some(mls::history(self).await?) } else { None },
        }));
        Ok(event)
    }

    fn invite_key(&self, code: &str) -> Vec<u8> {
        // Never use a default cell, caller-supplied namespace, or unscoped key.
        Subspace::all()
            .subspace(&("zoen", self.cell))
            .pack(&("inv", code_hash(code)))
    }

    async fn stored_invite(&self, code: &str) -> Result<StoredInvite> {
        let key = self.invite_key(code);
        let bytes = self
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move { Ok(trx.get(&key, false).await?.map(|v| v.to_vec())) }
            })
            .await?
            .context("native invite row missing")?;
        Ok(unpack(&bytes)?)
    }

    async fn mint(
        &mut self,
        who: &Author,
        max_uses: u32,
        ttl_secs: u64,
        expired_fixture: bool,
    ) -> Result<String> {
        let role = Role::Member;
        let created = self
            .log
            .create_invite(&who.identity, &self.space, role, max_uses, ttl_secs)
            .await
            .map_err(anyhow::Error::msg)?;
        let mut stored = self.stored_invite(&created.code).await?;
        let (bounded_uses, ttl_ms) = invite_limits(max_uses, ttl_secs);
        ensure!(stored.0 == self.space && stored.1 == "Member" && stored.2 == who.identity);
        ensure!(stored.3 == created.expires_at_ms && stored.4 == bounded_uses && stored.5 == 0);
        if expired_fixture {
            // Surgical test setup: only this freshly minted invite in our fresh
            // cell is backdated. The Worker mints the same already-expired row
            // using the equivalent historical clock below; no extra replay op.
            stored.3 = now_ms()? - 1;
            let key = self.invite_key(&created.code);
            let value = pack(&stored);
            self.db
                .run(|trx, _| {
                    let (key, value) = (key.clone(), value.clone());
                    async move {
                        trx.set(&key, &value);
                        Ok(())
                    }
                })
                .await?;
            stored = self.stored_invite(&created.code).await?;
        }
        let minted_now = stored
            .3
            .checked_sub(ttl_ms)
            .context("invite clock underflow")?;
        ensure!(minted_now >= 0);
        self.steps.push(json!({
            "op": "invite",
            "input": { "who": who.identity, "code": created.code, "role": role,
                "max_uses": max_uses, "ttl_secs": ttl_secs.to_string(), "now_ms": minted_now.to_string() },
            "expected": { "code_hash": code_hash(&created.code), "space": stored.0,
                "role": role, "created_by": stored.2, "expires_ms": stored.3.to_string(),
                "max_uses": stored.4, "uses": stored.5 },
            "native_stored_tuple": stored, "native_expired_fixture": expired_fixture,
        }));
        Ok(created.code)
    }
}

async fn fixture(log: &FdbLog, cell: &str, cluster: Option<&str>) -> Result<Value> {
    let db = Database::new(cluster)?;
    log.owner
        .maintain()
        .await
        .context("initialize isolated cell owner")?;
    let (owner, admin, member, reader, unregistered, joiner, exhausted) = (
        author(7, 9),
        author(11, 13),
        author(17, 19),
        author(23, 29),
        author(31, 37),
        author(41, 43),
        author(47, 53),
    );
    let mut journey = Journey {
        log,
        db: &db,
        cell,
        space: types::new_id("sp"),
        head: None,
        steps: Vec::new(),
    };
    let create = journey.signed(
        &owner,
        1,
        EventBody::SpaceCreated {
            title: "Public closed-group differential fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    );
    journey
        .publish("create closed Group", &owner, create, None, Expected::New)
        .await?;
    for (who, role, number) in [
        (&admin, Role::Admin, 2),
        (&member, Role::Member, 3),
        (&reader, Role::Reader, 4),
    ] {
        let env = journey.signed(
            &owner,
            number,
            EventBody::MemberAdded {
                identity: who.identity.clone(),
                role,
            },
        );
        journey
            .publish("owner adds role", &owner, env, Some(true), Expected::New)
            .await?;
    }
    let env = journey.signed(&reader, 5, message(5, "Reader cannot publish"));
    journey
        .publish(
            "reader denial",
            &reader,
            env,
            None,
            Expected::Rejected("readers can't write here"),
        )
        .await?;
    let env = journey.signed(
        &owner,
        6,
        EventBody::MemberAdded {
            identity: unregistered.identity.clone(),
            role: Role::Member,
        },
    );
    journey
        .publish(
            "unregistered target denial",
            &owner,
            env,
            Some(false),
            Expected::Rejected("that person isn't on Zoen yet"),
        )
        .await?;

    let original = journey.signed(&member, 7, message(7, "Original stored member message"));
    let receipt = journey
        .publish(
            "member message",
            &member,
            original.clone(),
            None,
            Expected::New,
        )
        .await?
        .context("new publication needs a receipt")?;
    let env = journey.signed(
        &owner,
        8,
        EventBody::MemberRemoved {
            identity: member.identity.clone(),
        },
    );
    journey
        .publish(
            "removal includes departed member in audience",
            &owner,
            env,
            None,
            Expected::New,
        )
        .await?;
    ensure!(log.role(&journey.space, &member.identity).await?.is_none());
    let replay = journey
        .publish(
            "removed author replays original receipt",
            &member,
            original.clone(),
            None,
            Expected::Duplicate,
        )
        .await?
        .context("duplicate needs original receipt")?;
    ensure!(
        replay == receipt,
        "native replay must preserve original bytes"
    );
    let changed = journey.signed(
        &member,
        7,
        message(7, "Different valid signed retry payload"),
    );
    ensure!(
        changed != original,
        "retry fixture must change signed bytes"
    );
    let replay = journey
        .publish(
            "removed author changes valid signed retry bytes",
            &member,
            changed,
            None,
            Expected::Duplicate,
        )
        .await?
        .context("changed retry needs original receipt")?;
    ensure!(
        replay == receipt,
        "changed retry must return original native receipt"
    );
    let env = journey.signed(
        &member,
        9,
        message(9, "Removed member cannot publish fresh events"),
    );
    journey
        .publish(
            "removed author fresh publish denial",
            &member,
            env,
            None,
            Expected::Rejected("not a member of this space"),
        )
        .await?;

    // Zero inputs deliberately exercise the shared lower bounds: one use, 60s.
    let code = journey.mint(&owner, 0, 0, false).await?;
    let mut join = journey.signed(
        &joiner,
        10,
        EventBody::MemberAdded {
            identity: joiner.identity.clone(),
            role: Role::Member,
        },
    );
    join.invite = Some(code.to_lowercase());
    journey
        .publish(
            "lowercase bounded invite join",
            &joiner,
            join.clone(),
            Some(true),
            Expected::New,
        )
        .await?;
    ensure!(journey.stored_invite(&code).await?.5 == 1);
    journey
        .publish(
            "invite retry does not spend another use",
            &joiner,
            join,
            Some(true),
            Expected::Duplicate,
        )
        .await?;
    ensure!(journey.stored_invite(&code).await?.5 == 1);
    let mut join = journey.signed(
        &exhausted,
        11,
        EventBody::MemberAdded {
            identity: exhausted.identity.clone(),
            role: Role::Member,
        },
    );
    join.invite = Some(code.clone());
    journey
        .publish(
            "invite use exhaustion",
            &exhausted,
            join,
            Some(true),
            Expected::Rejected("invite expired or already used"),
        )
        .await?;
    ensure!(journey.stored_invite(&code).await?.5 == 1);

    let expired_code = journey.mint(&owner, 1, 60, true).await?;
    let mut join = journey.signed(
        &exhausted,
        12,
        EventBody::MemberAdded {
            identity: exhausted.identity.clone(),
            role: Role::Member,
        },
    );
    join.invite = Some(expired_code.clone());
    journey
        .publish(
            "targeted expired invite denial",
            &exhausted,
            join,
            Some(true),
            Expected::Rejected("invite expired or already used"),
        )
        .await?;
    ensure!(journey.stored_invite(&expired_code).await?.5 == 0);
    // Negative cases stay outside the oracle timeline. Unsupported families and
    // malformed signatures must not perturb the projection or any durable row.
    let encrypted_genesis = Envelope::plain(&owner.sign_event(
        &types::new_id("sp"),
        &format!("{:026}", 100),
        1_700_000_000_100,
        None,
        EventBody::SpaceCreated {
            title: "Unsupported Public fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Public,
        },
    ));
    let encrypt = Envelope::plain(&owner.sign_event(
        &types::new_id("sp"),
        "00000000000000000000000101",
        1_700_000_000_101,
        None,
        EventBody::SpaceCreated {
            title: "Unsupported Community".into(),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
    ));
    let joining = journey.signed(
        &owner,
        102,
        EventBody::DeviceJoining {
            device: owner.device.clone().context("fixture needs owner device")?,
        },
    );
    // Reuse a real stored CID: signature verification must precede dedupe lookup.
    let mut invalid_signature = original;
    invalid_signature.sig = "00".repeat(64);
    let unsupported_reason = "only Direct and Group chats with Closed or EndToEnd privacy are supported by this private projection";
    let unsupported = vec![
        negative_case(
            "Public Group genesis stays unsupported",
            &owner,
            encrypted_genesis,
            "unsupported_projection",
            false,
            unsupported_reason,
        )?,
        negative_case(
            "Community genesis stays unsupported",
            &owner,
            encrypt,
            "unsupported_projection",
            false,
            unsupported_reason,
        )?,
        negative_case(
            "DeviceJoining on Closed Group is rejected",
            &owner,
            joining,
            "admission",
            true,
            "devices join groups of end-to-end spaces only",
        )?,
        negative_case(
            "invalid signature before stored-CID replay",
            &member,
            invalid_signature,
            "bad_signature",
            true,
            "",
        )?,
    ];
    let encrypted = mls::cases(log, &db, cell, cluster).await?;
    let authorized_reads = mls::read_cases(log, &db, cell).await?;
    Ok(
        json!({ "space_id": journey.space, "steps": journey.steps, "unsupported": unsupported,"encrypted":encrypted,"authorized_reads":authorized_reads }),
    )
}

fn main() -> Result<()> {
    // The network outlives every database handle and the Tokio runtime.
    let network = unsafe { foundationdb::boot() };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        // Neither command-line arguments nor environment variables can select the
        // namespace to clear. Only FDB_CLUSTER_FILE selects the existing cluster.
        let cell = types::new_id("qa_mls_v105");
        ensure!(cell.starts_with("qa_mls_v105_"), "owned QA cell scope");
        let cluster = std::env::var("FDB_CLUSTER_FILE").ok();
        let log = FdbLog::open_as(
            cluster.as_deref(),
            &cell,
            "cloudflare-closed-fixture",
            Duration::from_secs(30),
        )?;
        eprintln!("native closed fixture isolated cell: {cell}");
        let result = fixture(&log, &cell, cluster.as_deref()).await;
        // Always attempt cleanup after ordinary errors, including failed setup or
        // unexpected oracle outcomes. A cleanup failure suppresses JSON success.
        let cleanup = async {
            log.drop_cell().await?;
            mls::ensure_empty(cluster.as_deref(), &cell).await
        }
        .await;
        match (result, cleanup) {
            (Ok(value), Ok(())) => {
                eprintln!("native closed fixture cleaned cell: {cell}");
                Ok(value)
            }
            (Err(error), Ok(())) => Err(error.context(format!("isolated cell {cell} cleaned"))),
            (result, Err(error)) => Err(anyhow!(
                "isolated cell {cell} cleanup failed: {error}; fixture error: {:?}",
                result.err()
            )),
        }
    });
    drop(runtime);
    drop(network);
    println!("{}", serde_json::to_string(&result?)?);
    Ok(())
}
