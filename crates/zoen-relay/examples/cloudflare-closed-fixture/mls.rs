//! Actual MLS ciphertext and native FDB state for the private adapter oracle.
use super::*;
use foundationdb::RangeOption;
use roda_log::content::InnerEvent;
use roda_mls::{sealed::state_key, Device, Opened};
use roda_proto::{Sealed, SealedKind, STALE_COMMIT, STALE_SEAL};
use std::collections::BTreeSet;

pub(super) struct Tracking {
    epoch: Option<(u64, String)>,
    holds: Vec<Value>,
    pruned: u64,
}
impl Tracking {
    pub fn value(&self) -> Value {
        json!({"mls":self.epoch.as_ref().map(|(epoch,device)|json!({"next_epoch":epoch.to_string(),"last_device":device})),
            "holds":self.holds,"pruned":self.pruned.to_string()})
    }
    pub fn hold_clock(&self) -> Option<i64> {
        self.holds
            .iter()
            .filter_map(|value| value["when_ms"].as_str()?.parse().ok())
            .max()
    }
}

pub(super) async fn tracking(j: &Journey<'_>) -> Result<Tracking> {
    ensure!(
        j.cell.starts_with("qa_mls_v105_"),
        "only the owned MLS fixture cell is readable"
    );
    let root = Subspace::all().subspace(&("zoen", j.cell, "s", j.space.as_str()));
    let (epoch, holds, more, pruned) =
        j.db.run(|trx, _| {
            let root = root.clone();
            async move {
                let epoch = trx
                    .get(&root.pack(&"mls"), false)
                    .await?
                    .map(|value| value.to_vec());
                let mut range = RangeOption::from(root.subspace(&"ck").range());
                range.limit = Some(128);
                let page = trx.get_range(&range, 1, false).await?;
                let rows = page
                    .iter()
                    .map(|kv| (kv.key().to_vec(), kv.value().to_vec()))
                    .collect::<Vec<_>>();
                let pruned = trx
                    .get(&root.pack(&"pruned"), false)
                    .await?
                    .map(|value| value.to_vec());
                Ok((epoch, rows, page.more(), pruned))
            }
        })
        .await?;
    ensure!(!more, "the fixture never truncates native device holds");
    let epoch = epoch
        .map(|bytes| unpack::<(i64, String)>(&bytes).map(|(epoch, device)| (epoch as u64, device)))
        .transpose()?;
    let mut values = vec![];
    for (key, bytes) in holds {
        let (identity, device): (String, String) = root.subspace(&"ck").unpack(&key)?;
        let (seq, when): (i64, i64) = unpack(&bytes)?;
        values.push(json!({"identity":identity,"device":device,"seq":(seq as u64).to_string(),"when_ms":when.to_string()}));
    }
    let pruned = pruned
        .map(|bytes| unpack::<i64>(&bytes).map(|value| value as u64))
        .transpose()?
        .unwrap_or(0);
    Ok(Tracking {
        epoch,
        holds: values,
        pruned,
    })
}

pub(super) async fn history(j: &Journey<'_>) -> Result<Value> {
    let mut out = vec![];
    let mut from = 0;
    loop {
        let page = j.log.read(&j.space, from, 256).await?;
        let count = page.len();
        for event in page {
            from = event.seq + 1;
            out.push(json!({"seq":event.seq.to_string(),"prev":event.prev,"hash":event.hash,"bytes":STANDARD.encode(event.encode())}));
        }
        if count < 256 {
            break;
        }
    }
    Ok(json!(out))
}

pub(super) async fn ensure_empty(cluster: Option<&str>, cell: &str) -> Result<()> {
    ensure!(
        cell.starts_with("qa_mls_v105_"),
        "cleanup identity assertion"
    );
    let db = Database::new(cluster)?;
    let root = Subspace::all().subspace(&("zoen", cell));
    let empty = db
        .run(|trx, _| {
            let root = root.clone();
            async move {
                let mut range = RangeOption::from(root.range());
                range.limit = Some(1);
                let page = trx.get_range(&range, 1, false).await?;
                Ok(page.is_empty() && !page.more())
            }
        })
        .await?;
    ensure!(
        empty,
        "owned FDB prefix still contains fixture keys after cleanup"
    );
    Ok(())
}

struct Person {
    author: Author,
    db: rusqlite::Connection,
}
impl Person {
    fn new(identity: u8, device: u8) -> Result<Self> {
        let author = author(identity, device);
        let mut db = rusqlite::Connection::open_in_memory()?;
        roda_mls::migrate(&mut db)?;
        Ok(Self { author, db })
    }
    fn device(&self) -> Result<Device<'_>> {
        let secret = self.author.key.secret();
        Ok(Device::new(
            &self.db,
            state_key(&secret),
            &self.author.identity,
            secret,
            self.author.cert.as_deref().context("device certificate")?,
        )?)
    }
}
fn roster(people: &[&Person]) -> BTreeSet<String> {
    people.iter().map(|p| p.author.identity.clone()).collect()
}
fn sealed(j: &Journey<'_>, who: &Author, kind: SealedKind, data: Vec<u8>) -> Envelope {
    Envelope::sealed(
        who,
        &j.space,
        &format!("{:026}", j.steps.len() + 1000),
        1_700_000_000_000 + j.steps.len() as i64,
        j.head.as_ref(),
        Sealed::new(kind, roda_mls::SUITE_ID, data),
    )
}
async fn plain(
    j: &mut Journey<'_>,
    who: &Author,
    label: &str,
    body: EventBody,
    target: Option<bool>,
) -> Result<Sequenced> {
    let env = j.signed(who, j.steps.len() as u32 + 1000, body);
    accepted(j, who, label, env, target).await
}
async fn accepted(
    j: &mut Journey<'_>,
    who: &Author,
    label: &str,
    env: Envelope,
    target: Option<bool>,
) -> Result<Sequenced> {
    j.publish(label, who, env, target, Expected::New)
        .await?
        .context("new native receipt")
}
fn application(j: &Journey<'_>, who: &Person, text: &str) -> Result<(Envelope, Vec<u8>)> {
    let inner = who.author.sign_event(
        &j.space,
        &roda_types::new_ulid(now_ms()?),
        1_700_000_000_000 + j.steps.len() as i64,
        j.head.clone(),
        message(j.steps.len() as u32, text),
    );
    let encoded = InnerEvent {
        content: inner.content.clone(),
        sig: inner.sig.clone(),
    }
    .encode();
    let data = who.device()?.seal(&j.space, &encoded)?;
    let outer = Envelope::sealed(
        &who.author,
        &j.space,
        &inner.client_id,
        inner.at_ms,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, roda_mls::SUITE_ID, data),
    );
    Ok((outer, encoded))
}
fn open_application(
    receiver: &Person,
    sender: &Person,
    space: &str,
    env: &Envelope,
    people: &BTreeSet<String>,
    expected: &[u8],
) -> Result<()> {
    match receiver.device()?.open(
        space,
        env.sealed_data().context("MLS application bytes")?.1,
        people,
        sender.device()?.leaf(),
    )? {
        Opened::Application { plaintext, from } => {
            ensure!(
                plaintext == expected
                    && from.identity == sender.author.identity
                    && from.device == sender.author.device.clone().unwrap(),
                "actual MLS plaintext and certified sender"
            );
            let inner = InnerEvent::decode(&plaintext).context("actual core inner event")?;
            ensure!(
                inner.content == InnerEvent::decode(expected).unwrap().content,
                "exact signed content after MLS open"
            );
            Ok(())
        }
        other => Err(anyhow!("MLS application was not decrypted: {other:?}")),
    }
}
async fn checkpoint(j: &mut Journey<'_>, who: &Person, label: &str) -> Result<()> {
    let (epoch, digest) = who.device()?.checkpoint(&j.space)?;
    plain(
        j,
        &who.author,
        label,
        EventBody::Checkpoint {
            upto: j.head.clone().context("checkpoint head")?,
            epoch,
            digest,
        },
        None,
    )
    .await?;
    Ok(())
}

pub(super) async fn cases(
    log: &FdbLog,
    db: &Database,
    cell: &str,
    cluster: Option<&str>,
) -> Result<Vec<Value>> {
    let mut cases = vec![];
    for (kind, privacy) in [
        (SpaceKind::Direct, Privacy::Closed),
        (SpaceKind::Group, Privacy::EndToEnd),
    ] {
        let (owner, peer, newcomer) = (
            Person::new(61, 62)?,
            Person::new(63, 64)?,
            Person::new(65, 66)?,
        );
        let mut j = Journey {
            log,
            db,
            cell,
            space: roda_types::new_id("sp"),
            head: None,
            steps: vec![],
        };
        plain(
            &mut j,
            &owner.author,
            "actual encrypted chat genesis",
            EventBody::SpaceCreated {
                title: "Actual private MLS oracle".into(),
                kind,
                privacy,
            },
            None,
        )
        .await?;
        plain(
            &mut j,
            &owner.author,
            "known peer joins signed roster",
            EventBody::MemberAdded {
                identity: peer.author.identity.clone(),
                role: if kind == SpaceKind::Group {
                    Role::Admin
                } else {
                    Role::Member
                },
            },
            Some(true),
        )
        .await?;
        if privacy == Privacy::Closed {
            plain(
                &mut j,
                &owner.author,
                "Direct turns on end-to-end encryption",
                EventBody::SpaceEncrypted,
                None,
            )
            .await?;
        }
        let people = roster(&[&owner, &peer]);
        owner.device()?.create_group(&j.space)?;
        let first = owner.device()?.commit(
            &j.space,
            &peer.device()?.key_packages(1, false)?,
            &BTreeSet::new(),
        )?;
        ensure!(
            roda_mls::commit_epoch(&first.commit) == Some(0),
            "actual first commit epoch"
        );
        let env = sealed(&j, &owner.author, SealedKind::Commit, first.commit.clone());
        accepted(&mut j, &owner.author, "actual epoch-zero commit", env, None).await?;
        owner
            .device()?
            .open(&j.space, &first.commit, &people, owner.device()?.leaf())?;
        let welcome = first.welcome.context("actual first Welcome")?;
        let env = sealed(&j, &owner.author, SealedKind::Welcome, welcome.clone());
        accepted(
            &mut j,
            &owner.author,
            "own-device Welcome follows accepted commit",
            env,
            None,
        )
        .await?;
        ensure!(
            peer.device()?.join(&j.space, &welcome, &people)?,
            "actual recipient joins"
        );
        let (env, inner) = application(&j, &owner, "Private MLS message from the actual core")?;
        let first_application = env.clone();
        accepted(
            &mut j,
            &owner.author,
            "actual encrypted application",
            env.clone(),
            None,
        )
        .await?;
        open_application(&peer, &owner, &j.space, &env, &people, &inner)?;
        let clear = j.signed(
            &owner.author,
            9000,
            message(9000, "must never be stored in the clear"),
        );
        j.publish(
            "plaintext rejected after encryption",
            &owner.author,
            clear,
            None,
            Expected::Rejected(roda_proto::SEAL_REQUIRED),
        )
        .await?;

        let mut opened = 1;
        if kind == SpaceKind::Direct {
            // More than one complete native pruning batch, using actual MLS
            // ciphertext and successful peer decryption for every inner event.
            for n in 0..270 {
                if n % 50 == 0 {
                    log.owner.maintain().await?;
                }
                let (env, inner) =
                    application(&j, &owner, &format!("Synthetic encrypted history {n}"))?;
                accepted(
                    &mut j,
                    &owner.author,
                    "actual encrypted history application",
                    env.clone(),
                    None,
                )
                .await?;
                open_application(&peer, &owner, &j.space, &env, &people, &inner)?;
                opened += 1;
            }
            checkpoint(
                &mut j,
                &owner,
                "owner checkpoint retains unread peer history",
            )
            .await?;
            checkpoint(
                &mut j,
                &peer,
                "peer checkpoint prunes one bounded native batch",
            )
            .await?;
            let before = tracking(&j).await?;
            ensure!(
                before.pruned > 0 && before.pruned < j.head.as_ref().unwrap().seq,
                "bounded pruning leaves another batch"
            );
            j.publish(
                "pruning replay returns original native stub and advances remaining cursor",
                &owner.author,
                first_application,
                None,
                Expected::Duplicate,
            )
            .await?;
            ensure!(
                tracking(&j).await?.pruned > before.pruned,
                "duplicate completes safe native compaction"
            );
        } else {
            let second = Person::new(61, 67)?;
            plain(
                &mut j,
                &owner.author,
                "actual same-identity device starts a durable hold",
                EventBody::DeviceJoining {
                    device: second.author.device.clone().unwrap(),
                },
                None,
            )
            .await?;
            let addition = owner.device()?.commit(
                &j.space,
                &second.device()?.key_packages(1, false)?,
                &BTreeSet::new(),
            )?;
            let env = sealed(
                &j,
                &owner.author,
                SealedKind::Commit,
                addition.commit.clone(),
            );
            accepted(
                &mut j,
                &owner.author,
                "actual same-identity device addition commit",
                env,
                None,
            )
            .await?;
            owner
                .device()?
                .open(&j.space, &addition.commit, &people, owner.device()?.leaf())?;
            peer.device()?
                .open(&j.space, &addition.commit, &people, owner.device()?.leaf())?;
            let welcome = addition.welcome.context("actual second-device Welcome")?;
            let env = sealed(&j, &owner.author, SealedKind::Welcome, welcome.clone());
            accepted(
                &mut j,
                &owner.author,
                "actual second-device Welcome",
                env,
                None,
            )
            .await?;
            ensure!(
                second.device()?.join(&j.space, &welcome, &people)?,
                "actual second device joins"
            );
            let (stale_application, _) =
                application(&j, &owner, "Actual previous-epoch ciphertext")?;
            plain(
                &mut j,
                &owner.author,
                "third registered Group member",
                EventBody::MemberAdded {
                    identity: newcomer.author.identity.clone(),
                    role: Role::Member,
                },
                Some(true),
            )
            .await?;
            let people = roster(&[&owner, &peer, &newcomer]);
            let packages = newcomer.device()?.key_packages(1, false)?;
            let winning = peer
                .device()?
                .commit(&j.space, &packages, &BTreeSet::new())?;
            let losing = owner
                .device()?
                .commit(&j.space, &packages, &BTreeSet::new())?;
            let env = sealed(&j, &peer.author, SealedKind::Commit, winning.commit.clone());
            accepted(
                &mut j,
                &peer.author,
                "one real commit wins the current epoch race",
                env,
                None,
            )
            .await?;
            let stale = sealed(&j, &owner.author, SealedKind::Commit, losing.commit.clone());
            j.publish(
                "competing actual MLS commit is stale",
                &owner.author,
                stale.clone(),
                None,
                Expected::Rejected(STALE_COMMIT),
            )
            .await?;
            let wrong = sealed(
                &j,
                &owner.author,
                SealedKind::Welcome,
                losing.welcome.context("losing actual Welcome")?,
            );
            j.publish(
                "losing device cannot publish its Welcome",
                &owner.author,
                wrong,
                None,
                Expected::Rejected("a welcome follows its own commit"),
            )
            .await?;
            owner
                .device()?
                .open(&j.space, &winning.commit, &people, peer.device()?.leaf())?;
            peer.device()?
                .open(&j.space, &winning.commit, &people, peer.device()?.leaf())?;
            second
                .device()?
                .open(&j.space, &winning.commit, &people, peer.device()?.leaf())?;
            let welcome = winning.welcome.context("winning actual Welcome")?;
            let env = sealed(&j, &peer.author, SealedKind::Welcome, welcome.clone());
            accepted(
                &mut j,
                &peer.author,
                "winning device Welcome accepted",
                env,
                None,
            )
            .await?;
            ensure!(
                newcomer.device()?.join(&j.space, &welcome, &people)?,
                "actual third device joins"
            );
            j.publish(
                "actual old application epoch refused",
                &owner.author,
                stale_application,
                None,
                Expected::Rejected(STALE_SEAL),
            )
            .await?;
            let cold = FdbLog::open_as(
                cluster,
                cell,
                "cloudflare-closed-fixture",
                Duration::from_secs(30),
            )?;
            cold.owner.maintain().await?;
            ensure!(
                cold.append(&stale, true)
                    .await
                    .is_err_and(|error| error.reason == STALE_COMMIT),
                "cold native relay reloads current epoch"
            );
            let (env, inner) = application(&j, &peer, "Actual new epoch message")?;
            accepted(
                &mut j,
                &peer.author,
                "actual new epoch encrypted application",
                env.clone(),
                None,
            )
            .await?;
            open_application(&owner, &peer, &j.space, &env, &people, &inner)?;
            open_application(&newcomer, &peer, &j.space, &env, &people, &inner)?;
            open_application(&second, &peer, &j.space, &env, &people, &inner)?;
            opened += 3;
            plain(
                &mut j,
                &owner.author,
                "member removal drops every old hold",
                EventBody::MemberRemoved {
                    identity: newcomer.author.identity.clone(),
                },
                None,
            )
            .await?;
            let removed = sealed(
                &j,
                &newcomer.author,
                SealedKind::Application,
                newcomer
                    .device()?
                    .seal(&j.space, b"synthetic removed-device bytes")?,
            );
            j.publish(
                "removed member cannot append new ciphertext",
                &newcomer.author,
                removed,
                None,
                Expected::Rejected("not a member of this space"),
            )
            .await?;
            let removal = peer.device()?.commit(
                &j.space,
                &[],
                &BTreeSet::from([newcomer.author.identity.clone()]),
            )?;
            let env = sealed(&j, &peer.author, SealedKind::Commit, removal.commit.clone());
            accepted(&mut j, &peer.author, "actual MLS removal commit", env, None).await?;
            let remaining = roster(&[&owner, &peer]);
            owner
                .device()?
                .open(&j.space, &removal.commit, &remaining, peer.device()?.leaf())?;
            peer.device()?
                .open(&j.space, &removal.commit, &remaining, peer.device()?.leaf())?;
            second
                .device()?
                .open(&j.space, &removal.commit, &remaining, peer.device()?.leaf())?;
            checkpoint(&mut j, &owner, "Group owner checkpoint").await?;
            checkpoint(&mut j, &peer, "Group peer checkpoint").await?;
            checkpoint(
                &mut j,
                &second,
                "same-identity second-device checkpoint releases its independent hold",
            )
            .await?;
        }
        cases.push(json!({"space_id":j.space,"kind":kind,"initial_privacy":privacy,"steps":j.steps,
            "actual_peer_opens":opened,"cold_native_epoch_checked":kind==SpaceKind::Group,"history":history(&j).await?}));
    }
    Ok(cases)
}
