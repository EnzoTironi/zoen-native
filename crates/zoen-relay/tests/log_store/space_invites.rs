//! Management shares the actual invite/redemption authority, including pre-upgrade data.
use super::*;
use foundationdb::{
    tuple::{pack, Subspace},
    Database,
};
use protocol::space_invites::{Action, Rename, Request, SpaceInvitePage};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn manage(
    log: &FdbLog,
    who: &Author,
    space: &str,
    action: Action,
) -> TestResult<SpaceInvitePage> {
    Ok(log
        .manage_space_invites(
            &who.identity,
            &Request {
                space: space.into(),
                action: Some(action),
            },
        )
        .await?)
}

pub(super) async fn management_migrates_pages_and_revokes_against_actual_redemption() -> TestResult
{
    let cell = types::new_id("invite_management")?;
    let log = store_on(&cell);
    let owner = Author::root(Signer::generate()?);
    let member = Author::root(Signer::generate()?);
    let stranger = Author::root(Signer::generate()?);
    let (space, genesis) = create(&log, &owner).await?;
    let (other, _) = create(&log, &stranger).await?;
    let code = "ABCDEFGHJK";
    let id = zoen_relay::log::fdb::code_hash(code);
    let db = Database::new(std::env::var("FDB_CLUSTER_FILE").ok().as_deref())?;
    let root = Subspace::all().subspace(&("zoen", &cell));
    // Reproduce exact old schema: 258 records and no management index/checkpoint.
    // Backfill cannot silently show the first bounded slice as the complete list.
    db.run(|trx, _| {
        let root = root.clone();
        let space = space.clone();
        let owner = owner.identity.clone();
        let id = id.clone();
        async move {
            for i in 0..258 {
                let hash = if i == 0 {
                    id.clone()
                } else {
                    format!("{i:064x}")
                };
                trx.set(
                    &root.pack(&("inv", &hash)),
                    &pack(&(&space, "Member", &owner, now_ms() + 3600000, 10i64, 0i64)),
                );
            }
            Ok(())
        }
    })
    .await?;
    assert!(manage(&log, &stranger, &space, Action::List(String::new()))
        .await
        .is_err());
    let first = manage(&log, &owner, &space, Action::List(String::new())).await?;
    assert!(first.indexing && first.entries.is_empty() && first.next.is_empty());
    let mut page = manage(&log, &owner, &space, Action::List(String::new())).await?;
    assert!(!page.indexing);
    let mut records = page.entries.clone();
    while !page.next.is_empty() {
        page = manage(&log, &owner, &space, Action::List(page.next)).await?;
        records.extend(page.entries.clone());
    }
    assert_eq!(records.len(), 258);
    assert!(records.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert_eq!(records.iter().filter(|r| r.id == id).count(), 1);
    let updated = manage(
        &log,
        &owner,
        &space,
        Action::Rename(Rename {
            id: id.clone(),
            name: "Família".into(),
        }),
    )
    .await?;
    assert_eq!(updated.entries[0].name, "Família");
    assert!(manage(&log, &stranger, &other, Action::Revoke(id.clone()))
        .await
        .is_err());
    // Even an old link can still be used before the revoke; the index didn't spend it.
    log.preview_invite(code).await?;
    let revoked = manage(&log, &owner, &space, Action::Revoke(id.clone())).await?;
    assert!(revoked.entries[0].revoked);
    assert_eq!(revoked.entries[0].uses, 0);
    assert!(log.preview_invite(code).await.is_err());
    let mut join = sign(
        &member,
        &space,
        None,
        EventBody::MemberAdded {
            identity: member.identity.clone(),
            role: Role::Member,
        },
    )?;
    join.invite = Some(code.into());
    assert!(
        log.append(&join, true).await.is_err(),
        "revoked historical link cannot join"
    );
    assert_eq!(log.role(&space, &member.identity).await?, None);
    assert!(
        manage(&log, &owner, &space, Action::Revoke(id.clone()))
            .await?
            .entries[0]
            .revoked
    );
    // New mint after migration is indexed atomically, still storing only its hash.
    let fresh = log
        .create_invite(&owner.identity, &space, Role::Member, 10, 3600)
        .await?;
    let fresh_id = zoen_relay::log::fdb::code_hash(&fresh.code);
    let mut join = sign(
        &member,
        &space,
        None,
        EventBody::MemberAdded {
            identity: member.identity.clone(),
            role: Role::Member,
        },
    )?;
    join.invite = Some(fresh.code.clone());
    let revoke = manage(&log, &owner, &space, Action::Revoke(fresh_id.clone()));
    let (revoke, admitted) = tokio::join!(revoke, log.append(&join, true));
    let state = &revoke?.entries[0];
    assert_eq!(
        state.uses,
        u32::from(admitted.is_ok()),
        "join and revoke must serialize on authority key"
    );
    assert!(log.preview_invite(&fresh.code).await.is_err());
    // Removal cuts management even after an old owner-side review.
    let admin = Author::root(Signer::generate()?);
    log.append(
        &sign(
            &owner,
            &space,
            Some(genesis.clone()),
            EventBody::MemberAdded {
                identity: admin.identity.clone(),
                role: Role::Admin,
            },
        )?,
        true,
    )
    .await
    .map_err(|e| e.reason)?;
    manage(&log, &admin, &space, Action::List(String::new())).await?;
    let privileged = log
        .create_invite(&owner.identity, &space, Role::Admin, 1, 3600)
        .await?;
    assert!(manage(
        &log,
        &admin,
        &space,
        Action::Revoke(zoen_relay::log::fdb::code_hash(&privileged.code))
    )
    .await
    .is_err());
    log.append(
        &sign(
            &owner,
            &space,
            Some(genesis),
            EventBody::MemberRemoved {
                identity: admin.identity.clone(),
            },
        )?,
        true,
    )
    .await
    .map_err(|e| e.reason)?;
    assert!(manage(&log, &admin, &space, Action::List(String::new()))
        .await
        .is_err());
    // Cold storage client sees the same names/revocations; no local management ledger.
    let reopened = store_on(&cell);
    let mut recovered = manage(&reopened, &owner, &space, Action::List(String::new())).await?;
    let mut found = false;
    loop {
        if let Some(saved) = recovered.entries.iter().find(|entry| entry.id == id) {
            assert!(saved.revoked);
            assert_eq!(saved.name, "Família");
            found = true;
            break;
        }
        if recovered.next.is_empty() {
            break;
        }
        recovered = manage(&reopened, &owner, &space, Action::List(recovered.next)).await?;
    }
    assert!(
        found,
        "cold read must recover the existing label without rewriting it"
    );
    log.drop_cell().await?;
    Ok(())
}
