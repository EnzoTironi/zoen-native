use super::*;
use crate::sync::{AccountMeta, Ingest};
use roda_log::{Author, Signer};
use roda_proto::{Envelope, Sealed, SealedKind, Sequenced};
use std::collections::BTreeSet;

fn deliver(a: &mut Engine, b: &mut Engine, space: &str, env: Envelope) {
    let log = &a.logs[space];
    let seq = log.next_seq();
    let prev = log.head().unwrap().hash;
    let frame = Sequenced {
        seq,
        hash: roda_log::chain_hash(space, seq, &prev, &env.wire_hash()),
        prev,
        env,
    };
    assert!(matches!(
        a.ingest(frame.clone()),
        Ingest::Applied | Ingest::Confirmed
    ));
    assert!(matches!(
        b.ingest(frame),
        Ingest::Applied | Ingest::Confirmed
    ));
}

fn sealed(e: &Engine, space: &str, event: &Event) -> Envelope {
    let inner = roda_log::content::InnerEvent {
        content: event.content.clone(),
        sig: event.sig.clone(),
    };
    let bytes = e.device().unwrap().seal(space, &inner.encode()).unwrap();
    Envelope::sealed(
        e.net.author.as_ref().unwrap(),
        space,
        &event.client_id,
        event.at_ms,
        event.seen.as_ref(),
        Sealed::new(SealedKind::Application, roda_mls::SUITE_ID, bytes),
    )
}

#[test]
fn encrypted_conflicts_and_membership_changes_reconcile_before_the_own_approval_echo() {
    for removal in [false, true] {
        let path =
            std::env::temp_dir().join(format!("{}.sqlite", new_id("zoen_encrypted_approval")));
        let path = path.to_str().unwrap();
        let mut a = Engine::open(path).unwrap();
        let (root, a_secret, _) = a
            .create_account("Owner", "owner", "http://relay.test")
            .unwrap();
        let root = Signer::from_secret(&root);
        let owner = root.id();
        let agent = a
            .identities
            .values()
            .find(|i| i.kind == IdentityKind::Agent && i.owner.as_deref() == Some(&owner))
            .unwrap()
            .id
            .clone();
        let space = new_id("sp");
        a.append(
            &space,
            &owner,
            EventBody::SpaceCreated {
                title: "Encrypted approvals".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .unwrap();
        a.append(
            &space,
            &owner,
            EventBody::MemberAdded {
                identity: agent.clone(),
                role: Role::Member,
            },
        )
        .unwrap();
        a.set_trust(&agent, &space, TrustLevel::Suggest).unwrap();
        a.append(
            &space,
            &owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr"),
                    grantor: owner.clone(),
                    grantee: Some(agent.clone()),
                    scope: GrantScope::Everywhere,
                    capability: Capability::MonthlyBudget { cents: 1000 },
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
        a.agent_create_plan(
            &space,
            &agent,
            "Review the exact plan",
            plan(),
            "Scripted model",
            37,
        )
        .unwrap();
        let request = a.state.request_order[0].clone();
        let b_author = Author::device(&root, Signer::generate());
        let mut b = Engine::open(":memory:").unwrap();
        b.store.put_identity(&a.identities[&owner], None).unwrap();
        b.store.set_meta("me", &owner).unwrap();
        b.save_linked_account(AccountMeta {
            identity: owner.clone(),
            device: b_author.device.clone().unwrap(),
            cert: b_author.cert.clone().unwrap(),
            relay_url: "http://relay.test".into(),
            registered: true,
        })
        .unwrap();
        b.reload().unwrap();
        b.unlock(Some(b_author.key.secret().to_vec())).unwrap();
        b.put_profiles(vec![a.identities[&agent].clone()]).unwrap();
        for event in a.logs[&space].events().to_vec() {
            assert_eq!(
                b.ingest(Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: Envelope::plain(&event)
                }),
                Ingest::Applied
            );
        }
        a.store.set_synced(&space).unwrap();
        a.net.synced.insert(space.clone());
        let encrypted = a.append(&space, &owner, EventBody::SpaceEncrypted).unwrap();
        deliver(&mut a, &mut b, &space, Envelope::plain(&encrypted));
        // Both owner devices and the declared agent become actual MLS leaves.
        b.device().unwrap().forget(&space).unwrap();
        let b_package = b
            .device()
            .unwrap()
            .key_packages(1, false)
            .unwrap()
            .remove(0);
        let agent_author = Author::device(&a.signers[&agent], Signer::generate());
        let mut agent_store = roda_store::Store::in_memory().unwrap();
        roda_mls::migrate(agent_store.conn_mut()).unwrap();
        let agent_device = roda_mls::Device::new(
            agent_store.conn(),
            [77; 32],
            &agent,
            agent_author.key.secret(),
            agent_author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let agent_package = agent_device.key_packages(1, false).unwrap().remove(0);
        let commit = a
            .device()
            .unwrap()
            .commit(&space, &[b_package, agent_package], &BTreeSet::new())
            .unwrap();
        let env = Envelope::sealed(
            a.net.author.as_ref().unwrap(),
            &space,
            &new_ulid(crate::engine::now_ms()),
            crate::engine::now_ms(),
            a.logs[&space].head().as_ref(),
            Sealed::new(SealedKind::Commit, roda_mls::SUITE_ID, commit.commit),
        );
        deliver(&mut a, &mut b, &space, env);
        let welcome = commit.welcome.unwrap();
        let env = Envelope::sealed(
            a.net.author.as_ref().unwrap(),
            &space,
            &new_ulid(crate::engine::now_ms()),
            crate::engine::now_ms(),
            a.logs[&space].head().as_ref(),
            Sealed::new(SealedKind::Welcome, roda_mls::SUITE_ID, welcome.clone()),
        );
        deliver(&mut a, &mut b, &space, env);
        assert!(agent_device
            .join(
                &space,
                &welcome,
                &BTreeSet::from([owner.clone(), agent.clone()])
            )
            .unwrap());
        assert_eq!(a.device().unwrap().leaves(&space).unwrap().len(), 3);
        a.decide_request(&request, RequestDecision::AlwaysApprove)
            .unwrap();
        let pending_approval = a
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|p| matches!(p.event.body, EventBody::RequestResolved { .. }))
            .unwrap()
            .event;
        if removal {
            let removed = b
                .append(
                    &space,
                    &owner,
                    EventBody::MemberRemoved {
                        identity: agent.clone(),
                    },
                )
                .unwrap();
            deliver(&mut a, &mut b, &space, Envelope::plain(&removed));
        } else {
            b.resolve_request(&request, false).unwrap();
            let denied = b
                .store
                .outbox()
                .unwrap()
                .into_iter()
                .find(|p| matches!(p.event.body, EventBody::RequestResolved { .. }))
                .unwrap()
                .event;
            let env = sealed(&b, &space, &denied);
            deliver(&mut a, &mut b, &space, env);
        }
        let status = if removal {
            ReqStatus::Pending
        } else {
            ReqStatus::Denied
        };
        assert_eq!(a.state.requests[&request].status, status);
        assert!(a.items().is_empty());
        assert!(a.standing_decisions().is_empty());
        let env = sealed(&a, &space, &pending_approval);
        deliver(&mut a, &mut b, &space, env);
        for e in [&a, &b] {
            assert_eq!(e.state.requests[&request].status, status);
            assert!(e.items().is_empty());
            assert!(e.standing_decisions().is_empty());
            assert_eq!(e.agent_profile(&agent).budget_spent_cents, Some(37));
            assert_eq!(e.outbox_len(), 0);
            assert!(e.verify_all().iter().all(|r| r.valid));
        }
        let checkpoint = a.mls_status(&space);
        drop(a);
        let mut a = Engine::open(path).unwrap();
        a.unlock(Some(a_secret.to_vec())).unwrap();
        assert_eq!(a.state.requests[&request].status, status);
        assert!(a.items().is_empty());
        assert_eq!(a.mls_status(&space), checkpoint);
        assert_eq!(a.agent_profile(&agent).budget_spent_cents, Some(37));
        drop(a);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{path}{suffix}"));
        }
    }
}
