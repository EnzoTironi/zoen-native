//! Locale never changes a signed Page request's canonical prefix or visibility.
use super::*;
use crate::{Client, EntryKind, RequestStatus};
use std::path::{Path, PathBuf};
use types::{AgentRequest, ItemContent};

/// The exact pre-fix hash algorithm, with only its known synthesized first note supplied.
/// This independently preserves the old English and Portuguese wire vectors.
fn old_prefix(f: &PageFixture, creation_note: &str) -> String {
    let e = f.actor.agent.lock();
    let item = &e.state.items[&f.page];
    let rows: Vec<_> = item
        .versions
        .iter()
        .map(|v| {
            let ItemContent::Page(page) = &v.content else {
                panic!("ordinary Page fixture")
            };
            (
                v.number,
                &v.author,
                v.at_ms,
                if v.number == 1 {
                    creation_note
                } else {
                    v.note.as_str()
                },
                v.is_undo,
                page,
            )
        })
        .collect();
    tools::digest(&serde_json::to_vec(&(tools::REVISION, &item.space, &item.id, rows)).unwrap())
}
fn version_three(f: &PageFixture) {
    for i in 2..=3 {
        let current = page(f);
        let mut other = current.blocks.last().unwrap().clone();
        other.text = format!("Preserved human block {i}");
        f.actor
            .agent
            .page_apply_from(
                f.page.clone(),
                format!("human-version-{i}"),
                current.edit_context,
                current.blocks.iter().map(|b| b.id.clone()).collect(),
                vec![other],
            )
            .unwrap();
        // This is a signed user note, including its Portuguese text, not a synthesized label.
        f.actor
            .agent
            .page_commit(f.page.clone(), format!("Criado: signed publication {i}"))
            .unwrap();
    }
    assert_eq!(page(f).version, 3);
}
fn backup(
    f: &PageFixture,
    tag: &str,
) -> std::result::Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(types::new_id(tag)?);
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("state.sqlite");
    f.actor
        .agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    Ok((directory, path))
}
fn cold(f: &PageFixture, path: &Path, locale: &str) -> std::sync::Arc<Client> {
    let client = Client::open(path.to_str().unwrap().into(), locale.into()).unwrap();
    assert!(client.unlock(f.actor.agent_vault.clone()).unwrap());
    client
}
fn assert_request(client: &Client, f: &PageFixture, req: &AgentRequest) -> crate::AgentRequestDto {
    let projected = client
        .requests()
        .into_iter()
        .find(|r| r.id == req.id)
        .expect("original signed request must remain visible");
    assert_eq!(projected.status, RequestStatus::Pending);
    assert_eq!(
        projected.tool_approval.as_ref().unwrap().content_hash,
        req.content_hash
    );
    assert!(projected.detail.contains("Original text"));
    assert!(client.timeline(f.actor.space.clone()).unwrap().iter().any(
        |entry| matches!(&entry.kind, EntryKind::Request { request_id,title,approved:None }
            if request_id==&req.id && title==&req.title)
    ));
    assert_eq!(client.lock().state.requests[&req.id].req, *req);
    projected
}
fn original(f: &PageFixture, id: &str) -> AgentRequest {
    f.actor.agent.lock().state.requests[id].req.clone()
}
fn decide_on(client: &Client, f: &PageFixture, request: &AgentRequest) {
    let mut e = client.lock();
    let previous = e.me.replace(f.actor.owner.clone());
    let synced = e.net.synced.remove(&f.actor.space);
    let outcome = e.decide_tool_request(
        &request.id,
        &request.content_hash,
        ToolRequestDecision::ApproveOnce,
    );
    e.me = previous;
    if synced {
        e.net.synced.insert(f.actor.space.clone());
    }
    outcome.unwrap();
}

#[tokio::test]
async fn existing_english_request_remains_in_portuguese_native_command_timeline_after_cold_reopen()
{
    let f = fixture().unwrap();
    version_three(&f);
    let turn = turn(&f);
    let command = command(&turn);
    let staged = stage(&f, &turn, &command);
    let request = original(&f, &staged.id);
    let english = old_prefix(&f, "Created");
    assert_eq!(request.tool_proposal.as_ref().unwrap().sha256, english);
    assert_ne!(english, old_prefix(&f, "Criado"));
    let (directory, path) = backup(&f, "page-locale-en-pt").unwrap();
    let native = cold(&f, &path, "pt-BR");
    assert_eq!(
        native.lock().state.items[&f.page].versions[0].note,
        "Criado"
    );
    let dto = assert_request(&native, &f, &request);
    // Actual native command decoding/projection/serialization, not a constructed DTO.
    let json = native
        .client_request(
            json!({"op":"timeline","space_id":f.actor.space,"before":null,"limit":80}).to_string(),
        )
        .await
        .unwrap();
    let page: Value = serde_json::from_str(&json).unwrap();
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["kind"]["Request"]["request_id"].as_str() == Some(request.id.as_str())));
    assert_eq!(
        dto.tool_approval.as_ref().unwrap().content_hash,
        request.content_hash
    );
    let before = {
        let e = native.lock();
        (
            e.store.event_count().unwrap(),
            e.store.outbox_len().unwrap(),
        )
    };
    assert!(native.approved_page_tool_turn(&request.id).is_err());
    let e = native.lock();
    assert_eq!(
        (
            e.store.event_count().unwrap(),
            e.store.outbox_len().unwrap()
        ),
        before
    );
    drop(e);
    decide_on(&native, &f, &request);
    let approved = native.approved_page_tool_turn(&request.id).unwrap();
    let saved = native
        .agent_page_replace(approved.turn(), &command)
        .unwrap();
    assert_eq!(saved.observed_base_version, 3);
    assert_eq!(saved.current_version, 4);
    assert_eq!(
        native
            .page(f.page.clone())
            .unwrap()
            .blocks
            .iter()
            .find(|b| b.id == command.before.id),
        Some(&command.after)
    );
    let published = native.page(f.page.clone()).unwrap();
    for before in &turn.snapshot.blocks {
        assert_eq!(
            published.blocks.iter().find(|b| b.id == before.id),
            Some(if before.id == command.before.id {
                &command.after
            } else {
                before
            })
        );
    }
    let replay = native
        .agent_page_replace(approved.turn(), &command)
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.client_id, saved.client_id);
    assert_eq!(native.lock().state.requests[&request.id].req, request);
    drop(approved);
    drop(native);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn pre_fix_portuguese_signed_request_survives_english_refold_and_exact_approval_without_rewriting_it(
) {
    let f = fixture().unwrap();
    version_three(&f);
    // Save the real signed source/Page journal before the producer fixture opens a request.
    let (directory, path) = backup(&f, "page-locale-pt-en").unwrap();
    let turn = turn(&f);
    let mut command = command(&turn);
    let staged = stage(&f, &turn, &command);
    let mut request = original(&f, &staged.id);
    command.prefix_sha256 = old_prefix(&f, "Criado");
    let args = serde_json::to_value(&command).unwrap();
    let proposal = request.tool_proposal.as_mut().unwrap();
    proposal.sha256 = command.prefix_sha256.clone();
    proposal.arguments_json = miniapps::deckcraft::agent_tools::canonical_json(&args).unwrap();
    let mut continuation: Value = serde_json::from_str(&proposal.continuation_json).unwrap();
    continuation["call"]["arguments"] = args;
    proposal.continuation_json = continuation.to_string();
    request.content_hash = request.tool_proposal_hash(&f.actor.space).unwrap();
    let portuguese = cold(&f, &path, "pt-BR");
    // The baseline contains no request. Sign and append exactly the pre-fix Portuguese wire.
    assert!(portuguese.requests().is_empty());
    let agent = portuguese.lock().me_id().unwrap();
    portuguese
        .lock()
        .append(
            &f.actor.space,
            &agent,
            EventBody::RequestOpened {
                request: request.clone(),
            },
        )
        .unwrap();
    assert_request(&portuguese, &f, &request);
    drop(portuguese);
    let english = cold(&f, &path, "en");
    assert_request(&english, &f, &request);
    decide_on(&english, &f, &request);
    let approved = english.approved_page_tool_turn(&request.id).unwrap();
    assert_eq!(
        approved.turn().snapshot.prefix_sha256,
        command.prefix_sha256
    );
    let saved = english
        .agent_page_replace(approved.turn(), &command)
        .unwrap();
    assert_eq!(saved.current_version, 4);
    assert_eq!(english.lock().state.requests[&request.id].req, request);
    assert_eq!(english.requests().len(), 1);
    drop(approved);
    drop(english);
    let reopened = cold(&f, &path, "pt-PT");
    let approved = reopened.approved_page_tool_turn(&request.id).unwrap();
    let replay = reopened
        .agent_page_replace(approved.turn(), &command)
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.client_id, saved.client_id);
    assert_eq!(reopened.lock().state.requests[&request.id].req, request);
    drop(approved);
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn canonical_prefix_preserves_every_signed_later_note_and_refuses_arbitrary_legacy_labels() {
    let f = fixture().unwrap();
    version_three(&f);
    let english = old_prefix(&f, "Created");
    let portuguese = old_prefix(&f, "Criado");
    let unknown = old_prefix(&f, "arbitrary caller-provided label");
    let mut e = f.actor.agent.lock();
    for note in ["Created", "Criado"] {
        e.state.items.get_mut(&f.page).unwrap().versions[0].note = note.into();
        assert_eq!(tools::prefix(&e.state, &f.page, 3).unwrap(), english);
        assert!(tools::prefix_matches(&e.state, &f.page, 3, &english));
        assert!(tools::prefix_matches(&e.state, &f.page, 3, &portuguese));
        assert!(!tools::prefix_matches(&e.state, &f.page, 3, &unknown));
    }
    e.state.items.get_mut(&f.page).unwrap().versions[1]
        .note
        .push_str(" changed after signing");
    assert!(!tools::prefix_matches(&e.state, &f.page, 3, &english));
    assert!(!tools::prefix_matches(&e.state, &f.page, 3, &portuguese));
}
