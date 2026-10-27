//! Real FDB metadata/membership admission; no TURN secret or media service.
use super::*;

fn landed(result: Result<Sequencing, zoen_relay::log::Reject>) -> Sequenced {
    match result {
        Ok(Sequencing::New { ev, .. }) => ev,
        other => panic!("expected a fresh signed event: {other:?}"),
    }
}

pub(super) async fn turn_pair_requires_exact_current_encrypted_dm_writers(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let log = store()?;
    log.owner.maintain().await.unwrap();
    let ana = Author::device(&Signer::generate()?, Signer::generate()?);
    let bruno = Author::device(&Signer::generate()?, Signer::generate()?);
    for (kind, privacy, expected) in [
        (SpaceKind::Group, Privacy::EndToEnd, false),
        (SpaceKind::Direct, Privacy::Closed, false),
        (SpaceKind::Direct, Privacy::EndToEnd, true),
    ] {
        let space = types::new_id("turn-dm")?;
        let genesis = landed(
            log.append(
                &sign(
                    &ana,
                    &space,
                    None,
                    EventBody::SpaceCreated {
                        title: "Private transport fixture".into(),
                        kind,
                        privacy,
                    },
                )?,
                true,
            )
            .await,
        );
        let seen = Seen {
            seq: genesis.seq,
            hash: genesis.hash,
        };
        assert!(!log
            .call_pair(&space, &ana.identity, &bruno.identity)
            .await
            .unwrap());
        let add = |role| {
            sign(
                &ana,
                &space,
                Some(seen.clone()),
                EventBody::MemberAdded {
                    identity: bruno.identity.clone(),
                    role,
                },
            )
        };
        landed(log.append(&add(Role::Member)?, true).await);
        assert_eq!(
            log.call_pair(&space, &ana.identity, &bruno.identity)
                .await
                .unwrap(),
            expected
        );
        assert!(!log
            .call_pair(&space, &ana.identity, &ana.identity)
            .await
            .unwrap());
        assert!(!log
            .call_pair(&space, &ana.identity, &"ab".repeat(32))
            .await
            .unwrap());
        if expected {
            landed(log.append(&add(Role::Reader)?, true).await);
            assert!(!log
                .call_pair(&space, &ana.identity, &bruno.identity)
                .await
                .unwrap());
            landed(log.append(&add(Role::Member)?, true).await);
            assert!(log
                .call_pair(&space, &ana.identity, &bruno.identity)
                .await
                .unwrap());
            landed(
                log.append(
                    &sign(
                        &ana,
                        &space,
                        Some(seen),
                        EventBody::MemberRemoved {
                            identity: bruno.identity.clone(),
                        },
                    )?,
                    true,
                )
                .await,
            );
            assert!(!log
                .call_pair(&space, &ana.identity, &bruno.identity)
                .await
                .unwrap());
        }
    }
    assert!(!log
        .call_pair("missing", &ana.identity, &bruno.identity)
        .await
        .unwrap());
    log.drop_cell().await.unwrap();
    Ok(())
}
