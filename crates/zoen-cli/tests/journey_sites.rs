//! Production Core JSON port → real relay/directory → real publication process.
//! Inject one lost HTTP reply AFTER upstream acceptance; recover from the same vault.
mod common;
#[path = "journey_sites/proxy.rs"]
mod proxy;
#[path = "journey_sites/service.rs"]
mod service;

use client_core::{Client, CoreError, SecretVault};
use common::World;
use serde_json::{json, Value};
use sqlx::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};

struct Vault(PathBuf);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        std::fs::create_dir_all(&self.0).is_ok() && std::fs::write(self.0.join(key), value).is_ok()
    }
    fn delete(&self, key: String) {
        let _ = std::fs::remove_file(self.0.join(key));
    }
}
fn open(home: &Path) -> Arc<Client> {
    Client::open(
        home.join("zoen.sqlite").to_string_lossy().into(),
        "en-US".into(),
    )
    .unwrap()
}
async fn request(core: &Client, value: Value) -> Result<Value, CoreError> {
    let reply = core.client_request(value.to_string()).await?;
    Ok(serde_json::from_str(&reply).expect("genuine Core JSON response"))
}
async fn command(core: &Client, value: Value) -> Value {
    request(core, value).await.unwrap()
}
async fn settle(core: &Client) {
    core.wait_until_settled(12_000).await.unwrap();
}
async fn public(http: &reqwest::Client, url: &str, expected: Option<&str>) {
    let response = http.get(url).send().await.unwrap();
    match expected {
        Some(html) => {
            assert_eq!(response.status(), 200);
            let csp = response
                .headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                csp.contains("sandbox"),
                "public document must keep its opaque sandbox"
            );
            assert_eq!(response.text().await.unwrap(), html);
        }
        None => assert_eq!(response.status(), 404),
    }
}
async fn receipts(w: &World, expected: i64) {
    assert_eq!(
        w.count("SELECT count(*) FROM site_publication_receipts")
            .await,
        expected
    );
}
async fn review(core: &Client, scope: &Value, source: &Value, unpublish: bool) -> Value {
    command(
        core,
        json!({"op":"site_review","scope":scope,"source":source,"unpublish":unpublish}),
    )
    .await
}
async fn publish(core: &Client, review: &Value) -> Value {
    command(core, json!({"op":"site_publish","review":review})).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_site_version_survives_reply_loss_reopen_unpublish_and_explicit_rollback() {
    let (ports, management, public_origin) = service::Sites::reserve();
    let w = World::with_env(
        "sites_core",
        &[
            ("ZOEN_SITES_MANAGEMENT_ORIGIN", &management),
            ("ZOEN_SITES_PUBLIC_ORIGIN", &public_origin),
        ],
    )
    .await;
    let sites = service::Sites::start(&w, ports, management, public_origin).await;
    let proxy = sites.proxy().await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let home = w.dir.join("owner");
    std::fs::create_dir_all(&home).unwrap();
    let vault = Arc::new(Vault(home.join("vault")));
    let core = open(&home);
    core.create_account(
        "Site Owner".into(),
        "site_owner".into(),
        w.relay_url(),
        vault.clone(),
    )
    .unwrap();
    core.start_sync(None).unwrap();
    settle(&core).await;
    let account = core.account().unwrap();
    assert!(account.registered && account.unlocked);
    let space = core.create_group("Sites".into(), vec![]).unwrap();
    settle(&core).await;
    let scope =
        json!({"identity_id":account.identity_id,"device_id":account.device_id,"space_id":space});
    let html1 = "<!doctype html><meta charset=utf-8><h1>Chosen saved version — one</h1>";
    let html2 = "<!doctype html><meta charset=utf-8><h1>Newer private version — two</h1>";
    let first = command(
        &core,
        json!({"op":"site_create","scope":scope,"name":"journey.html",
        "html":html1,"mutation_id":"site_journey_create_1"}),
    )
    .await;
    settle(&core).await;
    let source1 = &first["source"];
    let initial = command(
        &core,
        json!({"op":"site_status","scope":scope,"source":source1}),
    )
    .await;
    assert!(initial["inspection"]["head"].is_null());
    assert!(initial["pending"].is_null());
    assert_eq!(
        initial["configuration"]["management_origin"],
        sites.management
    );
    let chosen_review = review(&core, &scope, source1, false).await;
    let site_id = chosen_review["mutation"]["site_id"].as_str().unwrap();
    let public_url = format!("{}/s/{site_id}", sites.public);
    public(&http, &public_url, None).await;
    let second = command(
        &core,
        json!({"op":"site_save","scope":scope,"source":source1,"html":html2}),
    )
    .await;
    let source2 = &second["source"];
    assert_ne!(source1["sha256"], source2["sha256"]);
    settle(&core).await;
    receipts(&w, 0).await;

    // Preparation must durably retain the exact consented intent without publishing it.
    let prepared = command(&core, json!({"op":"site_prepare","review":chosen_review})).await;
    assert_eq!(prepared["html"], html1);
    assert_eq!(prepared["signed"]["mutation"], chosen_review["mutation"]);
    receipts(&w, 0).await;
    public(&http, &public_url, None).await;
    let failure = request(&core, json!({"op":"site_publish","review":chosen_review})).await;
    assert!(
        failure.is_err(),
        "lost genuine reply must remain ambiguous to caller"
    );
    assert_eq!(
        proxy.dropped.load(Ordering::SeqCst),
        1,
        "fault must follow a genuine HTTP 200"
    );
    receipts(&w, 1).await;
    public(&http, &public_url, Some(html1)).await;
    core.stop_sync();
    drop(core);

    let core = open(&home);
    assert!(core.unlock(vault).unwrap());
    core.start_sync(None).unwrap();
    settle(&core).await;
    let recovered = command(
        &core,
        json!({"op":"site_status","scope":scope,"source":source2}),
    )
    .await;
    assert!(
        recovered["pending"].is_null(),
        "inspection must reconcile the encrypted pending intent"
    );
    assert_eq!(
        recovered["inspection"]["receipts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        recovered["inspection"]["receipts"][0]["request_id"],
        chosen_review["mutation"]["request_id"]
    );
    receipts(&w, 1).await;
    let latest = command(
        &core,
        json!({"op":"site_read","scope":scope,"item_id":source2["item_id"],
        "version":source2["version"],"expected_sha256":source2["sha256"]}),
    )
    .await;
    assert_eq!(
        latest["html"], html2,
        "publication never becomes private editing authority"
    );
    public(&http, &public_url, Some(html1)).await;

    let update = review(&core, &scope, source2, false).await;
    let receipt2 = publish(&core, &update).await;
    assert_eq!(receipt2["source"], *source2);
    receipts(&w, 2).await;
    public(&http, &public_url, Some(html2)).await;
    // Old intent is a retry, not rollback: it cannot move the public head backwards.
    let old = publish(&core, &chosen_review).await;
    assert_eq!(old["request_id"], chosen_review["mutation"]["request_id"]);
    receipts(&w, 2).await;
    public(&http, &public_url, Some(html2)).await;

    let remove = review(&core, &scope, source2, true).await;
    let removed = publish(&core, &remove).await;
    assert_eq!(removed["active"], false);
    receipts(&w, 3).await;
    public(&http, &public_url, None).await;
    let rollback = review(&core, &scope, source1, false).await;
    assert_ne!(
        rollback["mutation"]["request_id"],
        chosen_review["mutation"]["request_id"]
    );
    assert_eq!(rollback["mutation"]["expected_head"], removed["head"]);
    let restored = publish(&core, &rollback).await;
    assert_eq!(restored["source"], *source1);
    receipts(&w, 4).await;
    public(&http, &public_url, Some(html1)).await;
    let historical = command(
        &core,
        json!({"op":"site_read","scope":scope,"item_id":source1["item_id"],
        "version":source1["version"],"expected_sha256":source1["sha256"]}),
    )
    .await;
    assert_eq!(historical["current_version"], source2["version"]);
    assert_eq!(historical["editable"], false);
    assert_eq!(historical["publish_allowed"], true);

    // Revoke in this journey's actual directory, not in a mock client permission map.
    let refused = review(&core, &scope, source2, false).await;
    let mut db = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
    assert_eq!(
        sqlx::query("UPDATE devices SET revoked_at=now() WHERE identity=$1 AND device=$2")
            .bind(&account.identity_id)
            .bind(&account.device_id)
            .execute(&mut db)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
    assert!(
        request(&core, json!({"op":"site_publish","review":refused}))
            .await
            .is_err()
    );
    assert!(request(
        &core,
        json!({"op":"site_status","scope":scope,"source":source2})
    )
    .await
    .is_err());
    receipts(&w, 4).await;
    public(&http, &public_url, Some(html1)).await;
    core.stop_sync();
    drop(core);
    drop(db);
    drop(proxy);
    drop(sites);
}
