//! Role admission can precede the Agent's actual MLS reader membership.
use super::*;
use crate::app_binary_documents::AppBinaryDocumentDto;
use crate::app_edit_requests::AppEditRequestInput;
use base64::Engine as _;
use miniapps::{binary::Format, wordcraft};

const SPACE: &str = "app-request-missing-agent-package";

struct Fixture {
    owner: Engine,
    peers: [Engine; 2],
    agent_id: String,
    app: AppBinaryDocumentDto,
}

fn certified_agent(author: &Author, root: [u8; 32], handle: &str) -> R<Engine> {
    let mut agent = Engine::open(":memory:")?;
    let id = Signer::from_secret(&root).id();
    let proof = event_log::agent_owner::authorize(author, &id)
        .ok_or_else(|| storage("fixture needs a certified owner"))?;
    agent.create_agent_account(
        "App collaborator",
        handle,
        "http://relay.test",
        root,
        &author.identity,
        proof,
    )?;
    Ok(agent)
}

fn actors(ready_owned: bool) -> R<(Engine, [Engine; 2], String)> {
    let (mut owner, author, _) = member(":memory:");
    let reader = if ready_owned {
        certified_agent(&author, [95; 32], "ready_collaborator")?
    } else {
        member(":memory:").0
    };
    let agent = certified_agent(&author, [94; 32], "app_collaborator")?;
    let id = agent.me_id()?;
    for peer in [&reader, &agent] {
        let profile = peer
            .identities
            .get(&peer.me_id()?)
            .cloned()
            .ok_or_else(|| storage("fixture profile"))?;
        assert!(event_log::agent_owner::profile_authorized(&profile));
        owner.store.put_identity(&profile, None)?;
        owner.identity_order.push(profile.id.clone());
        owner.identities.insert(profile.id.clone(), profile);
    }
    owner.net.profiles.supported = true;
    Ok((owner, [reader, agent], id))
}

fn publish(owner: &mut Engine, peers: &mut [Engine; 2]) -> R<()> {
    let frames =
        relay_ready_to_peer(owner, &mut peers[0]).map_err(|error| storage(error.to_string()))?;
    for frame in frames {
        assert_eq!(peers[1].ingest(frame), Ingest::Applied);
    }
    Ok(())
}

fn create(owner: &mut Engine, space: &str, mutation: &str) -> R<AppBinaryDocumentDto> {
    owner.app_binary_create_format(
        Format::Wordcraft,
        space,
        "App reader control",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        mutation,
    )
}

fn post(
    owner: &mut Engine,
    space: &str,
    app: &AppBinaryDocumentDto,
    id: &str,
) -> R<crate::TimelineEntry> {
    post_mode(owner, space, app, id, false)
}
fn post_mode(
    owner: &mut Engine,
    space: &str,
    app: &AppBinaryDocumentDto,
    id: &str,
    share_current: bool,
) -> R<crate::TimelineEntry> {
    let account = owner.account().ok_or_else(|| storage("fixture account"))?;
    let input = AppEditRequestInput {
        target_agent_id: None,
        expected_target_revision: None,
        expected_resource_uri: None,
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: space.into(),
        item_id: app.item_id.clone(),
        expected_version: app.version,
        expected_sha256: app.sha256.clone(),
        text: "Format this current Word document".into(),
        mutation_id: id.into(),
        share_current,
    };
    owner.request_app_edit(input)
}

fn missing_package(ready_owned: bool) -> R<Fixture> {
    let (mut owner, mut peers, agent_id) = actors(ready_owned)?;
    let frames =
        claim_group(&mut owner, SPACE, &mut peers).map_err(|error| storage(error.to_string()))?;
    for frame in frames {
        for peer in &mut peers {
            assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
        }
    }
    let clock = protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or_else(|| storage("fixture claim clock"))?;
    let (_, targets, operation) = owner
        .mls_to_claim(Some(clock))?
        .ok_or_else(|| storage("fixture must actually request KeyPackages"))?;
    assert!(targets.contains(&agent_id));
    // Genuine package for the old reader, none for the certified Agent.
    let records = claimed_records(&mut peers[..1], &targets);
    assert_eq!(records.len(), 1);
    owner.mls_claimed(SPACE, &operation, Ok(records));
    assert!(owner.net.mls.stuck.contains(SPACE));
    publish(&mut owner, &mut peers)?;
    assert!(owner.ready_epoch(SPACE).is_some());
    assert!(peers[0].device()?.has_group(SPACE));
    assert!(!peers[1].device()?.has_group(SPACE));
    assert!(owner.state.writer(SPACE, &agent_id));
    let (_, _, readers) = owner
        .mls_status(SPACE)
        .ok_or_else(|| storage("old readers retain their real MLS group"))?;
    assert!(!readers.contains(&agent_id));
    let app = create(&mut owner, SPACE, "before_agent_welcome")?;
    publish(&mut owner, &mut peers)?;
    assert!(peers[0].item_state(&app.item_id).is_ok());
    assert!(peers[1].item_state(&app.item_id).is_err());
    Ok(Fixture {
        owner,
        peers,
        agent_id,
        app,
    })
}

#[test]
fn app_request_refuses_owned_role_member_missing_real_mls_keypackage() -> R<()> {
    let mut f = missing_package(false)?;
    for (locale, reason) in [
        ("en", "Wait for your agent to join this conversation."),
        ("pt", "Aguarde a entrada do seu agente na conversa."),
    ] {
        let _lang = crate::i18n::enter(crate::i18n::Lang::from_tag(locale));
        let before = (f.owner.store.event_count()?, f.owner.store.outbox()?.len());
        let result = post(&mut f.owner, SPACE, &f.app, "must_not_publish_old_epoch");
        assert!(
            matches!(&result, Err(CoreError::Stale { reason: actual }) if actual == reason),
            "{result:?}"
        );
        assert_eq!(
            (f.owner.store.event_count()?, f.owner.store.outbox()?.len()),
            before,
        );
    }
    Ok(())
}

#[test]
fn app_request_accepts_after_real_agent_welcome_and_retry_survives_removal() -> R<()> {
    let mut f = missing_package(false)?;
    // Make only the fixture scheduler's existing retry due, not MLS state.
    f.owner
        .net
        .mls
        .retry_at
        .insert(SPACE.into(), Instant::now());
    let clock = protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or_else(|| storage("fixture retry clock"))?;
    let (_, targets, operation) = f
        .owner
        .mls_to_claim(Some(clock))?
        .ok_or_else(|| storage("fixture must retry its missing Agent package"))?;
    assert_eq!(targets, vec![f.agent_id.clone()]);
    let records = claimed_records(&mut f.peers[1..], &targets);
    assert_eq!(records.len(), 1);
    f.owner.mls_claimed(SPACE, &operation, Ok(records));
    publish(&mut f.owner, &mut f.peers)?;
    assert!(f.peers[1].device()?.has_group(SPACE));
    let (_, _, readers) = f
        .owner
        .mls_status(SPACE)
        .ok_or_else(|| storage("MLS group"))?;
    assert!(readers.contains(&f.agent_id));
    let app = create(&mut f.owner, SPACE, "after_agent_welcome")?;
    publish(&mut f.owner, &mut f.peers)?;
    assert!(f.peers[1].item_state(&app.item_id).is_ok());
    let posted = post(&mut f.owner, SPACE, &app, "joined_request")?;
    publish(&mut f.owner, &mut f.peers)?;
    assert!(f.peers[1]
        .timeline(SPACE)?
        .iter()
        .any(|entry| entry.id == posted.id));
    f.owner.remove_member(SPACE, &f.agent_id)?;
    let before = (f.owner.store.event_count()?, f.owner.store.outbox()?.len());
    let retry = post(&mut f.owner, SPACE, &app, "joined_request")?;
    assert_eq!(retry.id, posted.id);
    assert_eq!(
        (f.owner.store.event_count()?, f.owner.store.outbox()?.len()),
        before,
    );
    Ok(())
}

#[test]
fn non_e2e_app_request_keeps_owned_writer_rule_without_mls_state() -> R<()> {
    for privacy in [Privacy::Closed, Privacy::Public] {
        let (mut owner, _, agent_id) = actors(false)?;
        let space =
            owner.create_synced_space("Non-MLS App", SpaceKind::Group, privacy, &[agent_id])?;
        assert!(owner.mls_status(&space).is_none());
        let app = create(&mut owner, &space, "non_mls_app")?;
        post(&mut owner, &space, &app, "non_mls_request")?;
    }
    Ok(())
}

#[test]
fn one_real_owned_mls_reader_allows_request_while_another_owned_agent_waits() -> R<()> {
    let mut f = missing_package(true)?;
    let ready_id = f.peers[0].me_id()?;
    let (_, _, readers) = f
        .owner
        .mls_status(SPACE)
        .ok_or_else(|| storage("MLS group"))?;
    assert!(readers.contains(&ready_id));
    assert!(!readers.contains(&f.agent_id));
    assert!(f.owner.state.writer(SPACE, &ready_id));
    assert!(f.owner.state.writer(SPACE, &f.agent_id));
    post(&mut f.owner, SPACE, &f.app, "one_owned_reader_is_enough")?;
    Ok(())
}

#[test]
fn disclosed_send_with_missing_keypackage_never_shares_or_appends_request() -> R<()> {
    let mut f = missing_package(false)?;
    let before = (
        f.owner.store.event_count()?,
        f.owner.outbox_len(),
        f.owner.state.grants.len(),
    );
    let result = post_mode(&mut f.owner, SPACE, &f.app, "disclosed_waiting", true);
    assert!(matches!(result, Err(crate::CoreError::Stale { .. })));
    assert_eq!(
        (
            f.owner.store.event_count()?,
            f.owner.outbox_len(),
            f.owner.state.grants.len()
        ),
        before
    );
    let a = f.owner.account().ok_or_else(|| storage("account"))?;
    let contexts = f
        .owner
        .member_candidates_mode(SPACE, "", &a.identity, &a.device, true)?;
    assert!(contexts
        .iter()
        .any(|c| c.member_id == f.agent_id && c.owned_agent && !c.delivery_ready));
    assert!(!contexts.iter().any(|c| c.delivery_ready));
    Ok(())
}

#[test]
fn typed_target_reports_real_mls_wait_and_never_shares_or_posts_before_the_welcome() -> R<()> {
    let mut f = missing_package(false)?;
    f.owner
        .set_trust(&f.agent_id, SPACE, types::TrustLevel::Suggest)?;
    let account = f.owner.account().ok_or_else(|| storage("owner account"))?;
    let mut input = AppEditRequestInput {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        item_id: f.app.item_id.clone(),
        expected_version: f.app.version,
        expected_sha256: f.app.sha256.clone(),
        text: "Edit after joining".into(),
        mutation_id: "typed_wait".into(),
        share_current: true,
        target_agent_id: Some(f.agent_id.clone()),
        expected_target_revision: None,
        expected_resource_uri: None,
    };
    let targets = f.owner.app_edit_targets(&(&input).into())?;
    let target = targets
        .agents
        .iter()
        .find(|a| a.agent.id == f.agent_id)
        .ok_or_else(|| storage("owned target"))?;
    assert!(!target.delivery_ready && !target.eligible);
    assert!(
        !target.permission_required,
        "Suggest exists, only the actual MLS reader is missing"
    );
    input.expected_target_revision = Some(targets.revision);
    let before = (
        f.owner.store.event_count()?,
        f.owner.outbox_len(),
        f.owner.state.grants.len(),
        f.owner.state.items.len(),
    );
    assert!(f.owner.request_app_edit(input).is_err());
    assert_eq!(
        (
            f.owner.store.event_count()?,
            f.owner.outbox_len(),
            f.owner.state.grants.len(),
            f.owner.state.items.len()
        ),
        before
    );
    Ok(())
}
