//! Real HTTP oracle, not an alternate admission model. Every fixture request goes
//! through the production router, SQL directory and object_store implementation.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{stream::BoxStream, FutureExt, TryStreamExt};
use object_store::{
    path::Path, CopyOptions, GetOptions, GetResult, GetResultPayload, ListResult, MultipartUpload,
    ObjectMeta, ObjectStore, ObjectStoreExt, PutMultipartOptions, PutOptions, PutPayload,
    PutResult,
};
use protocol::{auth_message, ClientFrame, Op};
use relay_admission::blobs as policy;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default)]
struct ControlledStore {
    inner: object_store::memory::InMemory,
    hold_next: AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    fail_put: AtomicBool,
    fail_get: AtomicBool,
    fail_read: AtomicBool,
}

impl std::fmt::Display for ControlledStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "isolated native media oracle")
    }
}

fn storage_failure() -> object_store::Error {
    object_store::Error::Generic {
        store: "native-media-fixture",
        source: std::io::Error::other("synthetic unavailable storage").into(),
    }
}

#[async_trait]
impl ObjectStore for ControlledStore {
    async fn put_opts(
        &self,
        key: &Path,
        body: PutPayload,
        options: PutOptions,
    ) -> object_store::Result<PutResult> {
        if self.hold_next.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if self.fail_put.load(Ordering::SeqCst) {
            return Err(storage_failure());
        }
        self.inner.put_opts(key, body, options).await
    }
    async fn put_multipart_opts(
        &self,
        key: &Path,
        options: PutMultipartOptions,
    ) -> object_store::Result<Box<dyn MultipartUpload>> {
        self.inner.put_multipart_opts(key, options).await
    }
    async fn get_opts(&self, key: &Path, options: GetOptions) -> object_store::Result<GetResult> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err(storage_failure());
        }
        let mut result = self.inner.get_opts(key, options).await?;
        if self.fail_read.load(Ordering::SeqCst) {
            result.payload =
                GetResultPayload::Stream(Box::pin(futures_util::stream::once(async {
                    Err(storage_failure())
                })));
        }
        Ok(result)
    }
    fn delete_stream(
        &self,
        keys: BoxStream<'static, object_store::Result<Path>>,
    ) -> BoxStream<'static, object_store::Result<Path>> {
        self.inner.delete_stream(keys)
    }
    fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        self.inner.list(prefix)
    }
    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> object_store::Result<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }
    async fn copy_opts(
        &self,
        from: &Path,
        to: &Path,
        options: CopyOptions,
    ) -> object_store::Result<()> {
        self.inner.copy_opts(from, to, options).await
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn objects(test: &TestRelay) -> Value {
    let mut rows = test
        .state
        .blobs
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    rows.sort_by(|a, b| a.location.cmp(&b.location));
    let mut values = Vec::new();
    // Query the underlying contents so each recorded hash is an independently
    // observed stored ciphertext digest, not just the requested object name.
    for row in rows {
        let bytes = test
            .state
            .blobs
            .get(&row.location)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        values.push(json!({"key":row.location.to_string(),"sha256":hex::encode(Sha256::digest(&bytes)),"length":bytes.len().to_string()}));
    }
    json!(values)
}

async fn expected(response: Response) -> Value {
    let status = response.status().as_u16();
    let mut headers = serde_json::Map::new();
    for key in [
        "content-type",
        "cache-control",
        "cross-origin-resource-policy",
        "access-control-allow-origin",
        "access-control-allow-methods",
        "access-control-allow-headers",
        "access-control-max-age",
        "retry-after",
    ] {
        if let Some(value) = response.headers().get(key) {
            headers.insert(key.into(), json!(value.to_str().unwrap()));
        }
    }
    assert!(response
        .headers()
        .get("access-control-allow-credentials")
        .is_none());
    json!({"status":status,"headers":headers,"body_base64":STANDARD.encode(response.bytes().await.unwrap())})
}

struct Fixture<'a> {
    test: &'a TestRelay,
    author: &'a Author,
    steps: Vec<Value>,
}

impl Fixture<'_> {
    async fn raw_put(&mut self, label: &str, sha: &str, body: &[u8], headers: Value, status: u16) {
        let clock = now();
        let mut request = self
            .test
            .client
            .put(format!("{}/v1/blobs/{sha}", self.test.url));
        for (key, value) in headers.as_object().unwrap() {
            request = request.header(key, value.as_str().unwrap());
        }
        let result = expected(request.body(body.to_vec()).send().await.unwrap()).await;
        assert_eq!(result["status"], status, "{label}");
        self.steps.push(json!({"label":label,"op":"put","sha":sha,"headers":headers,"body":small(body),
            "now_ms":clock.to_string(),"expected":result,"objects_after":objects(self.test).await,"device_active":true}));
    }
    async fn put(
        &mut self,
        label: &str,
        sha: &str,
        body: Vec<u8>,
        descriptor: Value,
        auth: (Option<String>, bool),
        status: u16,
    ) {
        let (timestamp, signature_valid) = auth;
        let clock = now();
        let ts = timestamp.unwrap_or_else(|| clock.to_string());
        let sig = if signature_valid {
            self.author.key.sign(&blob_put_message(
                sha,
                ts.parse().unwrap(),
                &self.test.state.relay_name,
            ))
        } else {
            String::new()
        };
        let device = self.author.device.as_deref().unwrap();
        let headers = json!({"x-zoen-device":device,"x-zoen-ts":ts,"x-zoen-sig":sig,"origin":"https://app.tryzoen.com"});
        let response = self
            .test
            .client
            .put(format!("{}/v1/blobs/{sha}", self.test.url))
            .header("origin", "https://app.tryzoen.com")
            .header("x-zoen-device", device)
            .header("x-zoen-ts", &ts)
            .header("x-zoen-sig", &sig)
            .body(body)
            .send()
            .await
            .unwrap();
        let result = expected(response).await;
        assert_eq!(result["status"], status, "{label}");
        self.steps.push(
            json!({"label":label,"op":"put","sha":sha,"headers":headers,"body":descriptor,
            "now_ms":clock.to_string(),"expected":result,"objects_after":objects(self.test).await,
            "device_active":db::device_active(&self.test.state.pool,device).await.unwrap()}),
        );
    }
    async fn get(&mut self, label: &str, sha: &str, status: u16) {
        self.get_path(label, sha, &format!("/v1/blobs/{sha}"), status)
            .await;
    }
    async fn get_path(&mut self, label: &str, sha: &str, path: &str, status: u16) {
        let response = self
            .test
            .client
            .get(format!("{}{path}", self.test.url))
            .header("origin", "https://app.tryzoen.com")
            .send()
            .await
            .unwrap();
        let result = expected(response).await;
        assert_eq!(result["status"], status, "{label}");
        self.steps.push(json!({"label":label,"op":"get","sha":sha,"path":path,"now_ms":now().to_string(),
            "expected":result,"objects_after":objects(self.test).await,
            "device_active":db::device_active(&self.test.state.pool,self.author.device.as_deref().unwrap()).await.unwrap()}));
    }
}

fn small(body: &[u8]) -> Value {
    json!({"base64":STANDARD.encode(body)})
}
fn repeat(length: usize) -> Value {
    json!({"repeat_byte":0,"length":length.to_string()})
}

async fn produce(test: &TestRelay, store: &ControlledStore) -> Value {
    let a = test.register().await;
    let profile: Value = sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
        .bind(&a.identity)
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    let nonce = "a7".repeat(32);
    let hello = ClientFrame::Hello {
        protocol: protocol::PROTOCOL_VERSION,
        capabilities: vec![],
        identity: a.identity.clone(),
        device: a.device.clone().unwrap(),
        cert: a.cert.clone().unwrap(),
    }
    .encode();
    let auth = ClientFrame::Auth {
        sig: a.key.sign(&auth_message(&nonce, &test.state.relay_name)),
    }
    .encode();
    let unlink = ClientFrame::Req {
        id: 7001,
        op: Op::Unlink {
            device: a.device.clone().unwrap(),
        },
    }
    .encode();
    let plain = b"synthetic encrypted media native oracle";
    let (key, sealed) = event_log::media::seal(plain);
    let sha = hex::encode(Sha256::digest(&sealed));
    let mut f = Fixture {
        test,
        author: &a,
        steps: vec![],
    };
    f.put(
        "bad-hash",
        "BAD",
        vec![],
        small(&[]),
        (Some(String::new()), false),
        400,
    )
    .await;
    f.put(
        "missing-signature",
        &sha,
        vec![],
        small(&[]),
        (Some(String::new()), false),
        401,
    )
    .await;
    f.put(
        "stale-signature",
        &sha,
        vec![],
        small(&[]),
        (Some("0".into()), false),
        401,
    )
    .await;
    f.put(
        "extreme-timestamp",
        &sha,
        vec![],
        small(&[]),
        (Some(i64::MIN.to_string()), false),
        401,
    )
    .await;
    f.put(
        "bad-signature-before-size",
        &sha,
        vec![],
        small(&[]),
        (None, false),
        401,
    )
    .await;
    f.put("empty", &sha, vec![], small(&[]), (None, true), 413)
        .await;
    f.get("missing", &sha, 404).await;
    f.put(
        "media-first",
        &sha,
        sealed.clone(),
        small(&sealed),
        (None, true),
        201,
    )
    .await;
    f.put(
        "duplicate",
        &sha,
        sealed.clone(),
        small(&sealed),
        (None, true),
        200,
    )
    .await;
    f.get("media-get", &sha, 200).await;
    let clock = now();
    let signed = a
        .key
        .sign(&blob_put_message(&sha, clock, &test.state.relay_name));
    let headers = json!({"x-zoen-device":a.device,"x-zoen-ts":clock.to_string(),"x-zoen-sig":signed,"origin":"https://app.tryzoen.com"});
    f.raw_put(
        "long-bad-hash",
        &"a".repeat(17 * 1024),
        &sealed,
        headers.clone(),
        400,
    )
    .await;
    let mut long_device = headers.clone();
    long_device["x-zoen-device"] = json!("a".repeat(32 * 1024));
    f.raw_put("long-bad-device", &sha, &sealed, long_device, 401)
        .await;
    let mut long_signature = headers.clone();
    long_signature["x-zoen-sig"] = json!("a".repeat(32 * 1024));
    f.raw_put("long-bad-signature", &sha, &sealed, long_signature, 401)
        .await;
    let mut long_timestamp = headers;
    long_timestamp["x-zoen-ts"] = json!(format!("{}{clock}", "0".repeat(17 * 1024)));
    f.raw_put(
        "long-leading-zero-timestamp",
        &sha,
        &sealed,
        long_timestamp,
        200,
    )
    .await;
    f.get_path(
        "percent-encoded-hash",
        &sha,
        &format!("/v1/blobs/%{:02X}{}", sha.as_bytes()[0], &sha[1..]),
        200,
    )
    .await;
    f.get_path("malformed-percent", "%ZZ", "/v1/blobs/%ZZ", 400)
        .await;
    f.get_path("invalid-utf8", "%FF", "/v1/blobs/%FF", 400)
        .await;
    f.get_path("encoded-slash", "/", "/v1/blobs/%2F", 400).await;
    let storage_failures = storage_failure_cases(test, store, &a, &sha, &sealed).await;
    let corrupt = vec![0; sealed.len()];
    f.put(
        "hash-mismatch",
        &sha,
        corrupt.clone(),
        small(&corrupt),
        (None, true),
        400,
    )
    .await;
    let max = vec![0; policy::MAX_BLOB_BYTES];
    let max_sha = hex::encode(Sha256::digest(&max));
    f.put(
        "max",
        &max_sha,
        max,
        repeat(policy::MAX_BLOB_BYTES),
        (None, true),
        201,
    )
    .await;
    f.put(
        "max-plus-one",
        &sha,
        vec![0; policy::MAX_BLOB_BYTES + 1],
        repeat(policy::MAX_BLOB_BYTES + 1),
        (None, true),
        413,
    )
    .await;
    f.put(
        "max-plus-one-auth-first",
        &sha,
        vec![0; policy::MAX_BLOB_BYTES + 1],
        repeat(policy::MAX_BLOB_BYTES + 1),
        (Some(String::new()), false),
        401,
    )
    .await;
    f.put(
        "extractor-limit-auth-first",
        &sha,
        vec![0; policy::MAX_BLOB_BYTES + 1024],
        repeat(policy::MAX_BLOB_BYTES + 1024),
        (Some(String::new()), false),
        401,
    )
    .await;
    f.put(
        "extractor-limit",
        &sha,
        vec![0; policy::MAX_BLOB_BYTES + 1025],
        repeat(policy::MAX_BLOB_BYTES + 1025),
        (Some(String::new()), false),
        413,
    )
    .await;
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(
        db::revoke_device(&mut tx, &a.identity, a.device.as_deref().unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    f.steps.push(json!({"label":"unlink","op":"revoke","now_ms":now().to_string(),
        "expected":{"device_active":false},"device_active":false,"objects_after":objects(test).await}));
    f.put(
        "revoked-put",
        &sha,
        sealed.clone(),
        small(&sealed),
        (None, true),
        403,
    )
    .await;
    f.put(
        "revoked-before-size",
        &sha,
        vec![],
        small(&[]),
        (None, true),
        403,
    )
    .await;
    f.get("capability-after-unlink", &sha, 200).await;
    let rate_vectors = [json!({"now_ns":"9007199254740993","tat_ns":null,"cost":policy::DEFAULT_BURST,"interval_ns":policy::DEFAULT_INTERVAL_NS.to_string(),"burst":policy::DEFAULT_BURST}),
        json!({"now_ns":"9007199254740993","tat_ns":(9_007_199_254_740_993_u64+policy::DEFAULT_INTERVAL_NS*u64::from(policy::DEFAULT_BURST)).to_string(),"cost":1,"interval_ns":policy::DEFAULT_INTERVAL_NS.to_string(),"burst":policy::DEFAULT_BURST})]
        .into_iter().map(|input| { let expected:Value=serde_json::from_str(&policy::rate_decision_json(&input.to_string()).unwrap()).unwrap();json!({"input":input,"expected":expected}) }).collect::<Vec<_>>();
    json!({"schema":1,"scope":"native-media-http","relay":test.state.relay_name,"identity":a.identity,
        "device":a.device,"certificate":a.cert,"profile":profile,"initial_registered":true,
        "quota":serde_json::from_str::<Value>(&policy::rate_config_json()).unwrap(),"steps":f.steps,
        "control":{"hello_base64":STANDARD.encode(hello),"nonce":nonce,"auth_base64":STANDARD.encode(auth),"unlink_frame_base64":STANDARD.encode(unlink)},
        "decryption":{"key_hex":hex::encode(key),"plain_base64":STANDARD.encode(plain)},"rate_vectors":rate_vectors,
        "storage_failures":storage_failures})
}

async fn storage_failure_cases(
    test: &TestRelay,
    store: &ControlledStore,
    a: &Author,
    sha: &str,
    body: &[u8],
) -> Vec<Value> {
    let before = objects(test).await;
    let clock = now();
    let signature = a
        .key
        .sign(&blob_put_message(sha, clock, &test.state.relay_name));
    let headers = json!({"x-zoen-device":a.device,"x-zoen-ts":clock.to_string(),"x-zoen-sig":signature,"origin":"https://app.tryzoen.com"});
    store.fail_put.store(true, Ordering::SeqCst);
    let put = test
        .client
        .put(format!("{}/v1/blobs/{sha}", test.url))
        .header("origin", "https://app.tryzoen.com")
        .header("x-zoen-device", a.device.as_deref().unwrap())
        .header("x-zoen-ts", clock.to_string())
        .header("x-zoen-sig", signature)
        .body(body.to_vec())
        .send()
        .await
        .unwrap();
    let put = expected(put).await;
    store.fail_put.store(false, Ordering::SeqCst);
    let mut cases = vec![
        json!({"fault":"put","op":"put","sha":sha,"body":small(body),"headers":headers,"now_ms":clock.to_string(),"expected":put}),
    ];
    for fault in ["get", "read"] {
        if fault == "get" {
            store.fail_get.store(true, Ordering::SeqCst);
        } else {
            store.fail_read.store(true, Ordering::SeqCst);
        }
        let response = test
            .client
            .get(format!("{}/v1/blobs/{sha}", test.url))
            .header("origin", "https://app.tryzoen.com")
            .send()
            .await
            .unwrap();
        let result = expected(response).await;
        store.fail_get.store(false, Ordering::SeqCst);
        store.fail_read.store(false, Ordering::SeqCst);
        cases.push(json!({"fault":fault,"op":"get","sha":sha,"now_ms":now().to_string(),"expected":result}));
    }
    for case in &mut cases {
        assert_eq!(case["expected"]["status"], 503);
        assert_eq!(
            case["expected"]["body_base64"],
            STANDARD.encode("storage unavailable")
        );
        case["objects_before"] = before.clone();
        case["objects_after"] = objects(test).await;
        assert_eq!(case["objects_before"], case["objects_after"]);
    }
    cases
}

#[tokio::test]
#[ignore = "isolated PostgreSQL native HTTP fixture; set ZOEN_TEST_PG and ZOEN_MEDIA_FIXTURE_OUTPUT"]
async fn cloudflare_media_native_fixture() {
    let output = std::env::var("ZOEN_MEDIA_FIXTURE_OUTPUT").expect("set owned fixture output path");
    let store = Arc::new(ControlledStore::default());
    let test = TestRelay::with_store(store.clone()).await;
    // Cleanup the freshly-created database even when an assertion/request fails.
    let result = std::panic::AssertUnwindSafe(produce(&test, &store))
        .catch_unwind()
        .await;
    test.close().await;
    let value = result.unwrap();
    std::fs::write(output, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[tokio::test]
async fn admitted_blob_put_can_finish_after_unlink_and_never_restores_device() {
    let store = Arc::new(ControlledStore::default());
    let test = TestRelay::with_store(store.clone()).await;
    let result = std::panic::AssertUnwindSafe(async {
        let a = test.register().await;
        let (_,body) = event_log::media::seal(b"admitted before unlink");
        let sha = hex::encode(Sha256::digest(&body));
        store.hold_next.store(true,Ordering::SeqCst);
        let path = format!("/v1/blobs/{sha}");
        let upload = test.signed(&a,Method::PUT,&path,&body,|ts| blob_put_message(&sha,ts,&test.state.relay_name));
        tokio::pin!(upload);
        tokio::select! {
            _ = store.entered.notified() => {},
            _ = &mut upload => panic!("held storage returned before release"),
            _ = tokio::time::sleep(Duration::from_secs(2)) => panic!("PUT did not reach admitted storage"),
        }
        let mut tx = test.state.pool.begin().await.unwrap();
        assert!(db::revoke_device(&mut tx,&a.identity,a.device.as_deref().unwrap()).await.unwrap());
        tx.commit().await.unwrap();
        assert!(!db::device_active(&test.state.pool,a.device.as_deref().unwrap()).await.unwrap());
        store.release.notify_one();
        assert_eq!(upload.await.status(),StatusCode::CREATED);
        let denied = test.signed(&a,Method::PUT,&format!("/v1/blobs/{sha}"),&body,|ts| blob_put_message(&sha,ts,&test.state.relay_name)).await;
        assert_eq!(denied.status(),StatusCode::FORBIDDEN);
        assert_eq!(denied.text().await.unwrap(),"unlinked device");
        let get = test.client.get(format!("{}/v1/blobs/{sha}",test.url)).send().await.unwrap();
        assert_eq!(get.status(),StatusCode::OK);
        assert_eq!(get.bytes().await.unwrap().as_ref(),body);
    }).catch_unwind().await;
    test.close().await;
    result.unwrap();
}

#[tokio::test]
async fn malformed_digest_and_duplicate_uploads_both_spend_device_quota() {
    let test = TestRelay::configured(
        Arc::new(object_store::memory::InMemory::new()),
        "blob_kib_device=1/h:3",
    )
    .await;
    let result = std::panic::AssertUnwindSafe(async {
        let a = test.register().await;
        let body = b"ciphertext quota fixture";
        let sha = hex::encode(Sha256::digest(body));
        for (payload, status) in [
            (b"incorrect".as_slice(), StatusCode::BAD_REQUEST),
            (body.as_slice(), StatusCode::CREATED),
            (body.as_slice(), StatusCode::OK),
        ] {
            let response = test
                .signed(
                    &a,
                    Method::PUT,
                    &format!("/v1/blobs/{sha}"),
                    payload,
                    |ts| blob_put_message(&sha, ts, &test.state.relay_name),
                )
                .await;
            assert_eq!(response.status(), status);
        }
        let refused = test
            .signed(&a, Method::PUT, &format!("/v1/blobs/{sha}"), body, |ts| {
                blob_put_message(&sha, ts, &test.state.relay_name)
            })
            .await;
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
        let seconds = refused.headers()["retry-after"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!((3590..=3600).contains(&seconds));
        let text = refused.text().await.unwrap();
        let wait = protocol::retry_hint(&text).expect("native shared slow-down message");
        assert!((3590..=3600).contains(&wait.as_secs()));
        let retained = test
            .client
            .get(format!("{}/v1/blobs/{sha}", test.url))
            .send()
            .await
            .unwrap();
        assert_eq!(retained.status(), StatusCode::OK);
        assert_eq!(retained.bytes().await.unwrap().as_ref(), body);
    })
    .catch_unwind()
    .await;
    test.close().await;
    result.unwrap();
}

#[tokio::test]
async fn create_only_concurrency_and_storage_failure_keep_media_contract() {
    let store = Arc::new(ControlledStore::default());
    let test = TestRelay::with_store(store.clone()).await;
    let result = std::panic::AssertUnwindSafe(async {
        let a = test.register().await;
        let (_, body) = event_log::media::seal(b"create once");
        let sha = hex::encode(Sha256::digest(&body));
        let path = format!("/v1/blobs/{sha}");
        let left = test.signed(&a, Method::PUT, &path, &body, |ts| {
            blob_put_message(&sha, ts, &test.state.relay_name)
        });
        let right = test.signed(&a, Method::PUT, &path, &body, |ts| {
            blob_put_message(&sha, ts, &test.state.relay_name)
        });
        let (a_response, b_response) = tokio::join!(left, right);
        let mut statuses = [a_response.status().as_u16(), b_response.status().as_u16()];
        statuses.sort();
        assert_eq!(statuses, [200, 201]);
        store.fail_put.store(true, Ordering::SeqCst);
        let failed = test
            .signed(&a, Method::PUT, &path, &body, |ts| {
                blob_put_message(&sha, ts, &test.state.relay_name)
            })
            .await;
        assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failed.text().await.unwrap(), "storage unavailable");
        store.fail_get.store(true, Ordering::SeqCst);
        let failed = test
            .client
            .get(format!("{}{}", test.url, path))
            .send()
            .await
            .unwrap();
        assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failed.text().await.unwrap(), "storage unavailable");
        store.fail_get.store(false, Ordering::SeqCst);
        let retained = test
            .client
            .get(format!("{}{}", test.url, path))
            .send()
            .await
            .unwrap();
        assert_eq!(retained.bytes().await.unwrap().as_ref(), body);
    })
    .catch_unwind()
    .await;
    test.close().await;
    result.unwrap();
}
