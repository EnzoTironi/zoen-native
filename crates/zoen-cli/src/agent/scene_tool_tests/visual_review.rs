//! Actual signed proposal -> human Core read-only preview, without another document.
use super::*;
use fixture::owner_change;
use serde_json::Value;
use sha2::{Digest, Sha256};

type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_returns_exact_bytes_and_refuses_foreign_binding() -> R<()> {
    let mut f = fixture().await?;
    let journal = f.path.join("visual-review.json");
    let cx = context(&f.client, &journal, &f.item, Arc::new(AtomicUsize::new(0)));
    canonical_tool_approval_tests::handle(&f.client, &cx, &f.space, &f.source).await;
    let request = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    drop(cx);
    let item = f.item.clone();
    let space = f.space.clone();
    let pin = f.resource_uri.clone();
    owner_change(&mut f, |owner| async move {
        let account = owner.account().unwrap();
        let before = owner.app_document_read(item.clone())?;
        let input = json!({"expected_identity_id":account.identity_id,
            "expected_device_id":account.device_id,"item_id":item,"space_id":space,
            "expected_resource_uri":pin,"request_id":request.id,"expected_hash":request.content_hash});
        let read = |input: serde_json::Value| json!({"op":"scene_tool_review","input":input}).to_string();
        let preview: Value = serde_json::from_str(&owner.client_request(read(input.clone())).await?)?;
        assert_eq!(preview["before_scene_json"], before.scene_json);
        assert_eq!(preview["before_sha256"], before.sha256);
        assert_eq!(preview["after_scene_json"], edited_scene());
        assert_eq!(preview["after_sha256"], hex::encode(Sha256::digest(edited_scene().as_bytes())));
        assert_eq!(preview["content_hash"], request.content_hash);
        assert_eq!(preview["resource_uri"], pin);
        assert_eq!(owner.app_document_read(item.clone())?, before, "preview is inert");
        for field in ["expected_identity_id","expected_device_id","item_id","space_id","expected_resource_uri","request_id","expected_hash"] {
            let mut changed = input.clone();
            changed[field] = Value::String("foreign".into());
            assert!(owner.client_request(read(changed)).await.is_err(), "accepted changed {field}");
        }
        let dto = owner.requests().into_iter().find(|r| r.id == request.id).unwrap();
        assert_eq!(dto.tool_approval.unwrap().preview_kind.as_deref(), Some("scene"));
        owner.decide_tool_request(request.id, request.content_hash, client_core::ToolRequestDecision::ApproveOnce)?;
        assert!(owner.client_request(read(input)).await.is_err(), "resolved preview cannot be reused");
        assert_eq!(owner.app_document_read(item)?, before, "rendering and approval do not execute the proposal");
        Ok(())
    }).await?;
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
