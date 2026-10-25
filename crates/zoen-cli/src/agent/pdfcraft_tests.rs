//! Real signed `PdfCraft` component, certified actor and native owner review.
//! Gate/brain/publisher are explicit offline fixtures; no inference or services.
#![warn(clippy::pedantic, clippy::perf)]
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::{deckcraft::DeckcraftWorker, Needs, ToolManifest};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const SOURCE: &[u8] = include_bytes!("../../../miniapps/src/pdfcraft/fixtures/actual-source.pdf");
const REVISION: &str = "d445410807e7eefbe5b96f9662ab963ae8dd2a3a";

fn worker() -> TestResult<Arc<DeckcraftWorker>> {
    let bytes = std::fs::read(std::env::var("ZOEN_TEST_PDFCRAFT_COMPONENT")?)?;
    assert!(!bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024);
    let key = SigningKey::from_bytes(&[42; 32]);
    let signed = ToolManifest {
        id: "pdfcraft-canonical-v1".into(),
        version: REVISION.into(),
        summary: "Pinned actual PdfCraft offline owner-review fixture".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hex::encode(Sha256::digest(&bytes))),
    }
    .sign(&key);
    let mut tampered = bytes.clone();
    tampered.push(0);
    assert!(
        DeckcraftWorker::load_for("pdfcraft", vec![key.verifying_key()], &signed, &tampered)
            .is_err()
    );
    assert!(
        DeckcraftWorker::load_for("photocraft", vec![key.verifying_key()], &signed, &bytes)
            .is_err()
    );
    Ok(Arc::new(DeckcraftWorker::load_for(
        "pdfcraft",
        vec![key.verifying_key()],
        &signed,
        &bytes,
    )?))
}

struct Review {
    path: std::path::PathBuf,
    vault: Arc<dyn client_core::SecretVault>,
    space: String,
    source: String,
    owner: String,
    base: client_core::AppBinaryDocumentDto,
    call: ToolCall,
    initial: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
    decisions: Arc<AtomicUsize>,
    journal: std::path::PathBuf,
    request: types::AgentRequest,
}
impl Review {
    fn context(&self, client: &Arc<Client>, worker: Arc<DeckcraftWorker>, resume: bool) -> Ctx {
        canonical_tool_approval_tests::context(
            client,
            worker,
            &self.journal,
            &self.call,
            &self.initial,
            &self.completed,
            &self.decisions,
            resume,
        )
    }
}

async fn stage(worker: Arc<DeckcraftWorker>) -> TestResult<Review> {
    let mut f = crate::deckcraft::tests::fixture();
    let space = f.client.item(f.base.item_id.clone())?.space_id;
    f.base = f.client.app_binary_create_for(
        "pdfcraft".into(),
        space.clone(),
        "Reviewed PDF".into(),
        STANDARD.encode(SOURCE),
        "real-pdf-worker".into(),
    )?;
    let inspected: serde_json::Value = serde_json::from_str(
        &crate::deckcraft::call(
            &f.client,
            &worker,
            &f.base.item_id,
            &f.source,
            &f.owner,
            "read_pdfcraft",
            &json!({"artifactId":f.base.item_id,"page":1,"offset":0}),
        )
        .await?,
    )?;
    assert_eq!(inspected["page"], 1);
    assert_eq!(inspected["pageCount"], 2);
    assert_eq!(inspected["query"], json!({"page":1,"offset":0}));
    let call = ToolCall {
        requires_review: false,
        id: "reviewed-pdf-rotation".into(),
        name: crate::deckcraft::model_name("pdfcraft_edit", &f.base.item_id)
            .ok_or("missing alias")?,
        arguments: json!({"artifactId":f.base.item_id,"version":f.base.version,"sha256":f.base.sha256,
            "action":{"op":"rotate_pages","pages":[1],"degrees":90}}),
    };
    let initial = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let decisions = Arc::new(AtomicUsize::new(0));
    let journal = f.path.join("pdf-tool-steps.json");
    let cx = canonical_tool_approval_tests::context(
        &f.client,
        worker.clone(),
        &journal,
        &call,
        &initial,
        &completed,
        &decisions,
        false,
    );
    canonical_tool_approval_tests::handle(&f.client, &cx, &space, &f.source).await;
    assert_eq!(initial.load(Ordering::SeqCst), 1);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    let requests = f.client.tool_approval_requests(&space, &f.source);
    assert_eq!(requests.len(), 1);
    let request = requests[0].clone();
    assert_eq!(
        request
            .tool_proposal
            .as_ref()
            .ok_or("missing proposal")?
            .canonical_tool,
        "pdfcraft_edit"
    );
    assert!(f
        .client
        .approved_tool_turn(&request.id, worker.binding_for("pdfcraft")?)
        .is_err());
    assert_eq!(
        f.client
            .app_binary_read(f.base.item_id.clone(), None, None)?,
        f.base
    );
    drop(cx);
    let crate::deckcraft::tests::Fixture {
        path,
        client,
        vault,
        owner,
        source,
        base,
        ..
    } = f;
    drop(client);
    Ok(Review {
        path,
        vault,
        space,
        source,
        owner,
        base,
        call,
        initial,
        completed,
        decisions,
        journal,
        request,
    })
}

async fn apply_review(
    r: &Review,
    worker: Arc<DeckcraftWorker>,
) -> TestResult<client_core::AppBinaryDocumentDto> {
    canonical_tool_approval_tests::signed_owner_resolution(&r.path, &r.space, &r.request, &r.owner);
    let client = canonical_tool_approval_tests::reopen(&r.path, r.vault.clone());
    let cx = r.context(&client, worker.clone(), true);
    let tools = SpaceTools::new(&client, &cx, &r.space, &r.owner, &r.source);
    assert!(tools.begin_resume(&r.request.id, &r.request.content_hash)?);
    drop(tools);
    drop(cx);
    let permit = client.approved_tool_turn(&r.request.id, worker.binding_for("pdfcraft")?)?;
    let receipt: serde_json::Value =
        serde_json::from_str(&crate::deckcraft::call_approved(&client, &worker, &permit).await?)?;
    assert_eq!(receipt["version"], 2);
    assert_eq!(receipt["authorId"], r.base.actor_id);
    assert_eq!(receipt["replayed"], false);
    let accepted = client.app_binary_read(r.base.item_id.clone(), None, None)?;
    let bytes = STANDARD.decode(&accepted.data_base64)?;
    assert_ne!(bytes, SOURCE);
    assert_eq!(hex::encode(Sha256::digest(&bytes)), accepted.sha256);
    assert_eq!(
        JournalState::load(&r.journal)?.steps,
        Vec::<(String, Vec<ToolStep>)>::new()
    );
    let out = std::path::PathBuf::from(std::env::var("ZOEN_TEST_PDF_OUTPUT")?);
    assert!(
        out.is_absolute(),
        "Explicit external output directory required"
    );
    std::fs::create_dir(&out)?;
    std::fs::write(out.join("accepted.pdf"), &bytes)?;
    std::fs::write(out.join("source.pdf"), SOURCE)?;
    std::fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(accepted)
}

async fn recover(
    r: &Review,
    worker: Arc<DeckcraftWorker>,
    accepted: &client_core::AppBinaryDocumentDto,
) -> TestResult {
    let client = canonical_tool_approval_tests::reopen(&r.path, r.vault.clone());
    let cx = r.context(&client, worker.clone(), true);
    canonical_tool_approval_tests::handle(&client, &cx, &r.space, &r.source).await;
    assert_eq!(r.initial.load(Ordering::SeqCst), 1);
    assert_eq!(r.completed.load(Ordering::SeqCst), 1);
    assert_eq!(
        &client.app_binary_read(r.base.item_id.clone(), None, None)?,
        accepted
    );
    assert_eq!(client.item(r.base.item_id.clone())?.versions.len(), 2);
    let steps = JournalState::load(&r.journal)?.steps;
    assert_eq!(steps.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&steps[0].1[0].result)?["replayed"],
        true
    );
    for (source, args) in [
        ("foreign-source", json!({"artifactId":r.base.item_id})),
        (r.source.as_str(), json!({"artifactId":"foreign-item"})),
        (
            r.source.as_str(),
            json!({"artifactId":r.base.item_id,"page":32}),
        ),
    ] {
        assert!(crate::deckcraft::call(
            &client,
            &worker,
            &r.base.item_id,
            source,
            &r.owner,
            "read_pdfcraft",
            &args
        )
        .await
        .is_err());
        assert_eq!(
            &client.app_binary_read(r.base.item_id.clone(), None, None)?,
            accepted
        );
    }
    drop(cx);
    drop(client);
    let client = canonical_tool_approval_tests::reopen(&r.path, r.vault.clone());
    assert_eq!(
        &client.app_binary_read(r.base.item_id.clone(), None, None)?,
        accepted
    );
    assert_eq!(
        client
            .app_binary_read(r.base.item_id.clone(), Some(1), Some(r.base.sha256.clone()))?
            .data_base64,
        STANDARD.encode(SOURCE)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "requires exact real PdfCraft WASIp2 and external output paths; no providers"]
async fn actual_pdf_owner_review_worker_cas_cold_retry_and_scoped_read() -> TestResult {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = worker()?;
    let review = stage(worker.clone()).await?;
    let accepted = apply_review(&review, worker.clone()).await?;
    recover(&review, worker, &accepted).await?;
    std::fs::remove_dir_all(&review.path)?;
    Ok(())
}
