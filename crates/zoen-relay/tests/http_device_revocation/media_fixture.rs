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

mod cases;
use cases::produce;

#[tokio::test]
async fn native_media_http_contract() {
    let store = Arc::new(ControlledStore::default());
    let test = TestRelay::with_store(store.clone()).await;
    // Cleanup the freshly-created database even when an assertion/request fails.
    let result = std::panic::AssertUnwindSafe(produce(&test, &store))
        .catch_unwind()
        .await;
    test.close().await;
    let value = result.unwrap();
    if let Ok(output) = std::env::var("ZOEN_MEDIA_FIXTURE_OUTPUT") {
        std::fs::write(output, serde_json::to_vec(&value).unwrap()).unwrap();
    }
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
