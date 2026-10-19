//! Actual native FDB visibility/read vectors with real core-generated MLS bytes.
//! This oracle never calls the new portable read-policy helper to obtain expected
//! permissions. It queries existing native spaces_of/removed_at/read independently.
use super::*;

const PAGE_BYTES: usize = 4 * 1024 * 1024;

async fn observe(
    j: &mut Journey<'_>,
    who: &Author,
    label: &str,
    from: u64,
    limit: usize,
) -> Result<Option<u64>> {
    ensure!((1..=256).contains(&limit), "bounded native read vector");
    let member = j.log.spaces_of(&who.identity).await?.contains(&j.space);
    let removed = j.log.removed_at(&j.space, &who.identity).await?;
    ensure!(!member || removed.is_none(), "native rejoin clears removal");
    let head = j.head.as_ref().map(|head| head.seq);
    let through = if member {
        head
    } else {
        removed.filter(|last| from <= *last)
    }
    .filter(|last| from <= *last);
    let mut events = vec![];
    let mut total = 0;
    let mut next = Some(from);
    if let Some(through) = through {
        // Native stores every ciphertext unaltered here (no checkpoint prunes it).
        // Apply the portable page's explicit transport byte bound to the exact
        // native bytes; permissions come only from native durable queries above.
        for event in j.log.read(&j.space, from, limit).await? {
            if event.seq > through {
                break;
            }
            let encoded = event.encode();
            if total + encoded.len() > PAGE_BYTES {
                break;
            }
            total += encoded.len();
            next = event.seq.checked_add(1);
            events.push(json!({"seq":event.seq.to_string(),"prev":event.prev,"hash":event.hash,"bytes":STANDARD.encode(encoded)}));
        }
    }
    let more = match (through, next) {
        (Some(through), Some(next)) if next <= through => j
            .log
            .read(&j.space, next, 1)
            .await?
            .first()
            .is_some_and(|event| event.seq <= through),
        _ => false,
    };
    ensure!(
        !more || !events.is_empty(),
        "byte bound must advance actual native entries"
    );
    j.steps.push(json!({"op":"read_for","label":label,"session":session(who)?,"from":from.to_string(),"limit":limit,
        "expected":{"events":events,"through":through.map(|value|value.to_string()),
            "visibility":{"member":member,"removed_at":removed.map(|value|value.to_string())},
            "has_more":more,"done":!more,"next_seq":if more {next.map(|value|value.to_string())}else{None}},
        "expected_bytes":total}));
    Ok(if more { next } else { None })
}

async fn complete_read(j: &mut Journey<'_>, who: &Author, label: &str) -> Result<usize> {
    let mut from = 0;
    let mut pages = 0;
    loop {
        let next = observe(j, who, label, from, 256).await?;
        pages += 1;
        ensure!(pages < 8, "bounded actual byte-cap pagination");
        match next {
            Some(next) => from = next,
            None => break,
        }
    }
    Ok(pages)
}

pub(crate) async fn cases(log: &FdbLog, db: &Database, cell: &str) -> Result<Vec<Value>> {
    ensure!(
        cell.starts_with("qa_mls_v105_"),
        "only owned read fixture cell"
    );
    let (owner, peer, stranger) = (
        Person::new(81, 82)?,
        Person::new(83, 84)?,
        Person::new(86, 87)?,
    );
    let mut j = Journey {
        log,
        db,
        cell,
        space: types::new_id("sp"),
        head: None,
        steps: vec![],
    };
    observe(
        &mut j,
        &stranger.author,
        "unknown Space cannot grant or bind a read",
        0,
        256,
    )
    .await?;
    plain(
        &mut j,
        &owner.author,
        "read Group encrypted genesis",
        EventBody::SpaceCreated {
            title: "Actual native visibility and page oracle".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        None,
    )
    .await?;
    plain(
        &mut j,
        &owner.author,
        "read peer added",
        EventBody::MemberAdded {
            identity: peer.author.identity.clone(),
            role: Role::Member,
        },
        Some(true),
    )
    .await?;
    owner.device()?.create_group(&j.space)?;
    let people = roster(&[&owner, &peer]);
    let initial = owner.device()?.commit(
        &j.space,
        &peer.device()?.key_packages(1, false)?,
        &BTreeSet::new(),
    )?;
    let env = sealed(
        &j,
        &owner.author,
        SealedKind::Commit,
        initial.commit.clone(),
    );
    accepted(
        &mut j,
        &owner.author,
        "read actual epoch-zero commit",
        env,
        None,
    )
    .await?;
    owner
        .device()?
        .open(&j.space, &initial.commit, &people, owner.device()?.leaf())?;
    let welcome = initial.welcome.context("actual read Welcome")?;
    let env = sealed(&j, &owner.author, SealedKind::Welcome, welcome.clone());
    accepted(
        &mut j,
        &owner.author,
        "read actual recipient Welcome",
        env,
        None,
    )
    .await?;
    ensure!(
        peer.device()?.join(&j.space, &welcome, &people)?,
        "actual read peer joins"
    );
    observe(&mut j, &peer.author, "entry limit continuation", 0, 2).await?;
    let mut opened = 0;
    // 64 real ~70KiB MLS applications cross 4MiB before the 256-entry limit.
    // Their exact signed envelopes still satisfy native 90KiB admission bounds.
    for n in 0..64 {
        let text = format!("Synthetic byte-cap application {n}: {}", "x".repeat(70_000));
        let (env, inner) = application(&j, &owner, &text)?;
        accepted(
            &mut j,
            &owner.author,
            "read actual large encrypted application",
            env.clone(),
            None,
        )
        .await?;
        open_application(&peer, &owner, &j.space, &env, &people, &inner)?;
        opened += 1;
    }
    let pages = complete_read(&mut j, &peer.author, "byte cap continuation before removal").await?;
    ensure!(
        pages > 1,
        "actual encrypted bytes must exceed one byte-bounded page"
    );
    observe(
        &mut j,
        &stranger.author,
        "unknown identity has no membership facts",
        0,
        256,
    )
    .await?;
    let past_head = j.head.as_ref().context("actual read head")?.seq + 1;
    observe(
        &mut j,
        &peer.author,
        "cursor beyond the head is terminal",
        past_head,
        256,
    )
    .await?;
    let removal = plain(
        &mut j,
        &owner.author,
        "read member removal records boundary",
        EventBody::MemberRemoved {
            identity: peer.author.identity.clone(),
        },
        None,
    )
    .await?;
    observe(
        &mut j,
        &peer.author,
        "removed member sees the removal event itself",
        removal.seq,
        256,
    )
    .await?;
    let removal_commit = owner.device()?.commit(
        &j.space,
        &[],
        &BTreeSet::from([peer.author.identity.clone()]),
    )?;
    let env = sealed(
        &j,
        &owner.author,
        SealedKind::Commit,
        removal_commit.commit.clone(),
    );
    accepted(
        &mut j,
        &owner.author,
        "read actual MLS member removal commit",
        env,
        None,
    )
    .await?;
    let remaining = roster(&[&owner]);
    owner.device()?.open(
        &j.space,
        &removal_commit.commit,
        &remaining,
        owner.device()?.leaf(),
    )?;
    let (env, _) = application(
        &j,
        &owner,
        "Actual post-removal ciphertext must not be disclosed",
    )?;
    accepted(
        &mut j,
        &owner.author,
        "read actual post-removal encrypted application",
        env,
        None,
    )
    .await?;
    observe(
        &mut j,
        &peer.author,
        "removed identity cannot read rows after removal",
        removal.seq + 1,
        256,
    )
    .await?;
    observe(
        &mut j,
        &peer.author,
        "removed identity prefix is inclusive despite newer ciphertext",
        removal.seq.saturating_sub(1),
        256,
    )
    .await?;
    observe(
        &mut j,
        &owner.author,
        "remaining member sees new encrypted rows",
        removal.seq,
        256,
    )
    .await?;
    // Rejoin the same identity with a real independent new device. History access
    // returns to native current-member semantics; MLS keys still require Welcome.
    let rejoined = Person::new(83, 85)?;
    plain(
        &mut j,
        &owner.author,
        "read same identity rejoins with a fresh device",
        EventBody::MemberAdded {
            identity: rejoined.author.identity.clone(),
            role: Role::Member,
        },
        Some(true),
    )
    .await?;
    observe(
        &mut j,
        &rejoined.author,
        "rejoin clears gone and restores current-member reads",
        removal.seq + 1,
        256,
    )
    .await?;
    let people = roster(&[&owner, &rejoined]);
    let added = owner.device()?.commit(
        &j.space,
        &rejoined.device()?.key_packages(1, false)?,
        &BTreeSet::new(),
    )?;
    let env = sealed(&j, &owner.author, SealedKind::Commit, added.commit.clone());
    accepted(
        &mut j,
        &owner.author,
        "read actual rejoin MLS commit",
        env,
        None,
    )
    .await?;
    owner
        .device()?
        .open(&j.space, &added.commit, &people, owner.device()?.leaf())?;
    let welcome = added.welcome.context("actual rejoin Welcome")?;
    let env = sealed(&j, &owner.author, SealedKind::Welcome, welcome.clone());
    accepted(
        &mut j,
        &owner.author,
        "read actual rejoin Welcome",
        env,
        None,
    )
    .await?;
    ensure!(
        rejoined.device()?.join(&j.space, &welcome, &people)?,
        "actual rejoined device obtains new epoch"
    );
    let (env, inner) = application(
        &j,
        &owner,
        "Actual freshly rejoined device decrypts current ciphertext",
    )?;
    accepted(
        &mut j,
        &owner.author,
        "read actual application after rejoin",
        env.clone(),
        None,
    )
    .await?;
    open_application(&rejoined, &owner, &j.space, &env, &people, &inner)?;
    opened += 1;
    complete_read(
        &mut j,
        &rejoined.author,
        "rejoined member byte-bounded full history",
    )
    .await?;
    observe(
        &mut j,
        &stranger.author,
        "stranger stays unauthorized after rejoin",
        0,
        256,
    )
    .await?;
    Ok(vec![
        json!({"space_id":j.space,"steps":j.steps,"actual_peer_opens":opened,"byte_limited_pages_before_removal":pages}),
    ])
}
