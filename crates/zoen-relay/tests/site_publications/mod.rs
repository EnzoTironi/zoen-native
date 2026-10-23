mod fences;
mod inspection;

use std::{path::Path, sync::Arc};

use event_log::{Author, Signer};
use object_store::{path::Path as ObjectPath, ObjectStore, ObjectStoreExt, PutPayload};
use protocol::sites::{Change, Mutation, Receipt, SignedMutation, SourceVersion, SCHEMA};
use sqlx::PgPool;
use types::{Identity, IdentityKind};
use zoen_relay::{
    db,
    sites::{management_router, public_router, Error, Service},
};

const NOW: i64 = 1_800_000_000_000;
const AUDIENCE: &str = "sites-test.example";
const PUBLIC_ORIGIN: &str = "https://published.example.test";
const FIRST: &[u8] = b"<!doctype html><title>Selected v1</title><p>First</p>";
const SECOND: &[u8] = b"<!doctype html><title>Selected v2</title><p>Second</p>";

pub async fn register(
    pool: &PgPool,
    root: &Signer,
    kind: IdentityKind,
    owner: Option<&Author>,
) -> Author {
    let author = Author::device(root, Signer::generate());
    let identity = Identity {
        id: root.id(),
        kind,
        owner: owner.map(|author| author.identity.clone()),
        name: "Site fixture".into(),
        handle: format!("site_{}", &root.id()[..12]),
        tint_hex: "#123456".into(),
        glyph: None,
        bio: String::new(),
        owner_proof: owner
            .map(|author| Box::new(event_log::agent_owner::authorize(author, &root.id()).unwrap())),
    };
    let mut tx = pool.begin().await.unwrap();
    db::register(
        &mut tx,
        &identity,
        &identity.handle,
        author.device.as_deref().unwrap(),
        author.cert.as_deref().unwrap(),
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    author
}

pub fn request(
    author: &Author,
    site: &str,
    previous: Option<String>,
    bytes: &[u8],
    version: u32,
) -> SignedMutation {
    let mut signed = SignedMutation {
        owner_id: author.identity.clone(),
        device_id: author.key.id(),
        at_ms: NOW,
        mutation: Mutation {
            schema: SCHEMA,
            public_origin: PUBLIC_ORIGIN.into(),
            site_id: site.into(),
            request_id: types::new_secret_id("pub"),
            expected_head: previous,
            change: Change::Publish {
                source: SourceVersion {
                    space_id: "private_space".into(),
                    item_id: "private_file".into(),
                    version,
                    sha256: event_log::sha256_hex(bytes),
                    byte_length: bytes.len() as u64,
                },
            },
        },
        signature: String::new(),
    };
    sign(author, &mut signed);
    signed
}

pub fn sign(author: &Author, signed: &mut SignedMutation) {
    signed.signature = author.key.sign(&signed.signing_bytes(AUDIENCE));
}

async fn head(pool: &PgPool, site: &str) -> (String, bool) {
    sqlx::query_as("SELECT head, active FROM site_publications WHERE site_id=$1")
        .bind(site)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn exercise(pool: &PgPool, directory: &Path) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let store: Arc<dyn ObjectStore> =
        Arc::new(object_store::local::LocalFileSystem::new_with_prefix(directory).unwrap());
    let config =
        zoen_relay::sites::configure("https://api.example.test", PUBLIC_ORIGIN, AUDIENCE).unwrap();
    let service = Service::new(pool.clone(), store.clone(), config.clone()).unwrap();
    let owner = Signer::generate();
    let author = register(pool, &owner, IdentityKind::Person, None).await;
    let site = types::new_secret_id("site");
    let first = request(&author, &site, None, FIRST, 1);
    let public = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let public_url = format!("http://{}", public.local_addr().unwrap());
    let management = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let management_url = format!("http://{}", management.local_addr().unwrap());
    let public_service = service.clone();
    let public_task = tokio::spawn(async move {
        axum::serve(public, public_router(public_service))
            .await
            .unwrap();
    });
    let api_service = service.clone();
    let api_task = tokio::spawn(async move {
        axum::serve(management, management_router(api_service))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let url = format!("{public_url}/s/{site}");
    assert_eq!(client.get(&url).send().await.unwrap().status(), 404);
    assert_eq!(
        service.apply(&first, Some(SECOND), NOW).await,
        Err(Error::Invalid)
    );
    assert_eq!(client.get(&url).send().await.unwrap().status(), 404);
    let (one, two) = tokio::join!(
        service.apply(&first, Some(FIRST), NOW),
        service.apply(&first, Some(FIRST), NOW)
    );
    let receipt = one.unwrap();
    assert_eq!(two.unwrap(), receipt);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM site_publication_receipts WHERE site_id=$1")
            .bind(&site)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let http_site = types::new_secret_id("site");
    let mut http_request = request(&author, &http_site, None, SECOND, 2);
    http_request.at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    sign(&author, &mut http_request);
    let response = client
        .post(format!("{management_url}/v1/sites/publications"))
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&serde_json::json!({
                "signed": http_request, "html": std::str::from_utf8(SECOND).unwrap()
            }))
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let http_receipt: Receipt = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(http_receipt, Receipt::for_mutation(&http_request, AUDIENCE));
    assert_eq!(
        client
            .get(format!("{public_url}/s/{http_site}"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        SECOND
    );
    let public = client.get(&url).send().await.unwrap();
    assert_eq!(public.status(), 200);
    assert_eq!(public.headers()["cache-control"], "no-store");
    assert_eq!(public.headers()["x-content-type-options"], "nosniff");
    let csp = public.headers()["content-security-policy"]
        .to_str()
        .unwrap();
    assert!(csp.contains("sandbox allow-scripts;"));
    assert!(!csp.contains("allow-same-origin"));
    assert!(csp.contains("connect-src 'none'"));
    assert!(public.headers().get("set-cookie").is_none());
    assert_eq!(public.bytes().await.unwrap().as_ref(), FIRST);
    assert_eq!(
        client
            .get(format!("{management_url}/s/{site}"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(format!("{public_url}/v1/sites/publications"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let second = request(&author, &site, Some(receipt.head.clone()), SECOND, 2);
    let next = service.apply(&second, Some(SECOND), NOW).await.unwrap();
    let mut stale = request(&author, &site, Some(receipt.head.clone()), FIRST, 1);
    assert_eq!(
        service.apply(&stale, Some(FIRST), NOW).await,
        Err(Error::Conflict)
    );
    assert_eq!(
        service.apply(&first, Some(FIRST), NOW).await.unwrap(),
        receipt
    );
    assert_eq!(head(pool, &site).await, (next.head.clone(), true));
    stale.mutation.request_id = second.mutation.request_id.clone();
    stale.mutation.expected_head = Some(next.head.clone());
    sign(&author, &mut stale);
    assert_eq!(
        service.apply(&stale, Some(FIRST), NOW).await,
        Err(Error::Conflict)
    );

    let outsider = register(pool, &Signer::generate(), IdentityKind::Person, None).await;
    let foreign = request(&outsider, &site, Some(next.head.clone()), FIRST, 1);
    assert_eq!(
        service.apply(&foreign, Some(FIRST), NOW).await,
        Err(Error::Unauthorized)
    );
    let agent = register(
        pool,
        &Signer::generate(),
        IdentityKind::Agent,
        Some(&author),
    )
    .await;
    let delegated = request(&agent, &types::new_secret_id("site"), None, FIRST, 1);
    assert_eq!(
        service.apply(&delegated, Some(FIRST), NOW).await,
        Err(Error::Unauthorized)
    );
    let mut removal = second.clone();
    removal.mutation.request_id = types::new_secret_id("pub");
    removal.mutation.expected_head = Some(next.head.clone());
    removal.mutation.change = Change::Unpublish;
    sign(&author, &mut removal);
    let unpublished = service.apply(&removal, None, NOW).await.unwrap();
    assert!(!unpublished.active);
    assert_eq!(client.get(&url).send().await.unwrap().status(), 404);
    assert_eq!(
        service.apply(&first, Some(FIRST), NOW).await.unwrap(),
        receipt
    );
    assert_eq!(client.get(&url).send().await.unwrap().status(), 404);
    let rollback = request(&author, &site, Some(unpublished.head), FIRST, 1);
    let restored = service.apply(&rollback, Some(FIRST), NOW).await.unwrap();
    // Fresh service object, same directory and database: no in-memory head state.
    let reopened = Service::new(pool.clone(), store.clone(), config).unwrap();
    assert_eq!(
        reopened.apply(&rollback, Some(FIRST), NOW).await.unwrap(),
        restored
    );
    assert_eq!(
        client
            .get(&url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        FIRST
    );

    let invalid = sqlx::query("INSERT INTO site_publications(site_id,owner_id,head,active,object_key,byte_length,content_sha256) VALUES ($1,$2,$3,true,'object',NULL,$3)")
        .bind(types::new_secret_id("site")).bind(owner.id()).bind("a".repeat(64)).execute(pool).await;
    assert!(
        matches!(invalid.unwrap_err(), sqlx::Error::Database(error) if error.code().as_deref() == Some("23514"))
    );
    // Corrupted retained object cannot produce a new success receipt.
    let corrupt_site = types::new_secret_id("site");
    let corrupt = request(&author, &corrupt_site, None, FIRST, 1);
    let key = format!(
        "public-sites/v1/{corrupt_site}/{}",
        event_log::sha256_hex(FIRST)
    );
    store
        .put(
            &ObjectPath::from(key),
            PutPayload::from_bytes(b"corrupted".to_vec().into()),
        )
        .await
        .unwrap();
    assert_eq!(
        service.apply(&corrupt, Some(FIRST), NOW).await,
        Err(Error::Unavailable)
    );
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM site_publications WHERE site_id=$1)")
            .bind(&corrupt_site)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(!exists);
    inspection::exercise(&service, &author, &outsider, &site, &restored).await;
    fences::exercise(pool, &service, &author).await;
    assert_eq!(
        service.apply(&rollback, Some(FIRST), NOW).await,
        Err(Error::Unauthorized)
    );
    // HTTP rejects a stale request with the same production signature policy.
    let response = client
        .post(format!("{management_url}/v1/sites/publications"))
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(
                &serde_json::json!({"signed":first,"html":std::str::from_utf8(FIRST).unwrap()}),
            )
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    public_task.abort();
    api_task.abort();
}
