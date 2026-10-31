//! Conversation publication reuses the canonical poll App across independent MLS peers.
use super::*;
use client_core::{MediaDraftScope, PrivacyDto};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_poll_reopens_publishes_once_with_replies_and_delivers_the_same_builtin_to_a_peer(
) -> TestResult {
    let (w, space) = enrolled("poll_reply_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let private =
        alice
            .0
            .create_group_with("Private poll source".into(), vec![], PrivacyDto::Closed)?;
    let secret = alice
        .0
        .send_message(private.clone(), "Alpha 😀 Beta".into())?;
    let root = bob.0.send_message(space.clone(), "Alpha 😀 Beta".into())?;
    received(&alice.0, &space, |rows| {
        rows.iter().any(|entry| entry.id == root.id)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    drop(alice);
    for mode in ["plain", "selected", "shared", "thread"] {
        let alice = Open::offline(&w, "alice")?;
        let account = alice.0.account().ok_or("account")?;
        let scope = MediaDraftScope {
            expected_identity_id: account.identity_id.clone(),
            expected_device_id: account.device_id.clone(),
            space_id: space.clone(),
            thread_root_id: (mode == "thread").then(|| root.id.clone()),
        };
        let mut value = serde_json::json!({"kind":"poll","reply_to":null,"question":format!("Question {mode} 雨"),"options":[{"id":"private_option_A","text":"First"},{"id":"private_option_B","text":"Second"}]});
        let source = if mode == "shared" { &secret } else { &root };
        let selection = serde_json::json!({"source_id":source.id,"revision":source.text_revision,"text":"😀","spans":[{"start_utf16":6,"end_utf16":8}]});
        if mode == "shared" {
            let review = serde_json::json!({"op":"review_shared_reply","expected_identity_id":account.identity_id,"expected_device_id":account.device_id,"space_id":space,"thread_root_id":null,"source_space_id":private,"message_id":secret.id,"expected_source_revision":secret.text_revision,"selection":selection});
            value["shared_reply"] =
                serde_json::from_str(&alice.0.client_request(review.to_string()).await?)?;
        } else if mode == "selected" {
            value["reply_to"] = root.id.clone().into();
            value["reply_quote"] = selection;
        }
        if matches!(mode, "selected" | "shared") {
            value["reply_intent"] = "private_poll_intent".into();
        }
        let initial = alice.0.media_draft(scope.clone())?;
        let before = alice.0.log_events(space.clone())?;
        let saved = alice
            .0
            .save_media_draft(scope.clone(), initial.revision, value.to_string(), vec![])?
            .draft;
        assert_eq!(
            alice.0.log_events(space.clone())?,
            before,
            "editing must not publish"
        );
        drop(alice);
        let alice = Open::offline(&w, "alice")?;
        assert_eq!(alice.0.media_draft(scope.clone())?, saved);
        let sent = alice
            .0
            .send_media_draft(scope.clone(), saved.revision.clone())?;
        assert_eq!(sent.entries.len(), 1);
        let id = sent.entries[0].id.clone();
        let accepted = alice.0.log_events(space.clone())?;
        drop(alice);
        let alice = Open::offline(&w, "alice")?;
        assert_eq!(
            alice
                .0
                .send_media_draft(scope.clone(), saved.revision)?
                .entries[0]
                .id,
            id
        );
        assert_eq!(
            alice.0.log_events(space.clone())?,
            accepted,
            "cold retry must not install or grant again"
        );
        alice.0.start_sync(None)?;
        alice.0.wait_until_settled(20_000).await?;
        let rows = received(&bob.0, &space, |rows| {
            rows.iter().any(|entry| entry.id == id)
        })
        .await?;
        assert_eq!(rows.iter().filter(|entry| entry.id == id).count(), 1);
        let delivered = find(&rows, &id)?;
        if mode == "thread" {
            assert_eq!(delivered.in_thread.as_deref(), Some(root.id.as_str()));
        } else if mode == "plain" {
            assert!(delivered.reply_to.is_none());
        } else {
            let reply = delivered.reply_to.as_ref().ok_or("reply")?;
            assert_eq!(reply.id, source.id);
            assert_eq!(reply.text, "😀");
            if mode == "shared" {
                let origin = reply.origin.as_ref().ok_or("private origin")?;
                assert_eq!(origin.space_id, private);
                assert!(!origin.available);
                assert!(!bob.0.spaces().iter().any(|s| s.id == private));
                assert!(bob.0.client_request(serde_json::json!({"op":"message","space_id":private,"message_id":secret.id}).to_string()).await.is_err());
            }
        }
        let EntryKind::Message {
            card: Some(card), ..
        } = delivered.kind
        else {
            return Err("poll card".into());
        };
        let detail = bob.0.item(card.item_id.clone())?;
        let app = detail.app.ok_or("replicated builtin")?;
        assert_eq!(app.app_id, "poll");
        let state: serde_json::Value = serde_json::from_str(&app.view_json)?;
        assert_eq!(state["question"], format!("Question {mode} 雨"));
        assert_eq!(state["options"][0]["text"], "First");
        assert!(!app.view_json.contains("private_option_"));
        assert!(!serde_json::to_string(&delivered.reply_to)?.contains("private_poll_intent"));
        // Preserve the existing voting path and its shared document authority.
        bob.0.app_call_tool(
            card.item_id.clone(),
            "poll_vote".into(),
            r#"{"option":"o1"}"#.into(),
            false,
        )?;
        bob.0.wait_until_settled(20_000).await?;
        received(&alice.0, &space, |_| {
            alice
                .0
                .item(card.item_id.clone())
                .ok()
                .and_then(|item| item.app)
                .is_some_and(|app| {
                    serde_json::from_str::<serde_json::Value>(&app.view_json)
                        .ok()
                        .is_some_and(|view| {
                            view["options"][0]["votes"] == serde_json::json!(["Bob"])
                        })
                })
        })
        .await?;
    }
    Ok(())
}
