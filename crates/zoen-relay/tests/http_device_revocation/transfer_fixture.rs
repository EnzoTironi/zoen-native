//! Native HTTP oracle for the private R2 transfer host. Real router, SQL and
//! ciphertext, with every response recorded before portable differential replay.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::TryStreamExt;
use object_store::ObjectStoreExt;
use protocol::{auth_message, ClientFrame, Op};
use serde_json::{json, Value};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
async fn inventory(test: &TestRelay) -> Value {
    let mut rows = test
        .state
        .blobs
        .list(None)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    rows.sort_by(|a, b| a.location.cmp(&b.location));
    let mut objects = vec![];
    for row in rows {
        let body = test
            .state
            .blobs
            .get(&row.location)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        objects.push(json!({"key":row.location.to_string(),"length":body.len().to_string(),"sha256":hex::encode(Sha256::digest(body))}));
    }
    json!(objects)
}
fn signed(a: &Author, op: &str, id: &str, chunk: &str, body: &[u8], ts: i64) -> Value {
    let sha = if op == "put" {
        hex::encode(Sha256::digest(body))
    } else {
        String::new()
    };
    json!({"origin":"https://app.tryzoen.com","x-zoen-device":a.device,"x-zoen-ts":ts.to_string(),
        "x-zoen-sig":a.key.sign(&transfer_message(op,id,chunk,&sha,ts))})
}
struct Oracle<'a> {
    test: &'a TestRelay,
    steps: Vec<Value>,
}
impl Oracle<'_> {
    async fn record(
        &mut self,
        label: &str,
        method: Method,
        path: &str,
        body: &[u8],
        headers: Value,
        status: u16,
    ) {
        let clock = now();
        let mut request = self
            .test
            .client
            .request(method.clone(), format!("{}{path}", self.test.url));
        for (key, value) in headers.as_object().unwrap() {
            request = request.header(key, value.as_str().unwrap());
        }
        let response = request.body(body.to_vec()).send().await.unwrap();
        let actual = response.status().as_u16();
        let mut response_headers = serde_json::Map::new();
        for key in [
            "content-type",
            "cache-control",
            "access-control-allow-origin",
            "access-control-allow-methods",
            "access-control-allow-headers",
            "access-control-max-age",
            "retry-after",
        ] {
            if let Some(value) = response.headers().get(key) {
                response_headers.insert(key.into(), json!(value.to_str().unwrap()));
            }
        }
        let response_body = STANDARD.encode(response.bytes().await.unwrap());
        assert_eq!(actual, status, "{label}: {response_body}");
        let descriptor = if body.len() > 2048 && body.iter().all(|v| *v == 0) {
            json!({"repeat_byte":0,"length":body.len().to_string()})
        } else {
            json!({"base64":STANDARD.encode(body)})
        };
        self.steps.push(json!({"label":label,"method":method.as_str(),"path":path,"body":descriptor,"headers":headers,
            "now_ms":clock.to_string(),"expected":{"status":actual,"headers":response_headers,"body_base64":response_body},"objects_after":inventory(self.test).await}));
    }
}

#[tokio::test]
async fn cloudflare_transfer_native_fixture() {
    let test = TestRelay::new().await;
    let a = test.register().await;
    let id = Signer::generate().id();
    let other = Signer::generate().id();
    let plain = b"synthetic encrypted device history with messages and pages";
    let (key, sealed) = event_log::media::seal(plain);
    assert_eq!(event_log::media::open(&key, &sealed).unwrap(), plain);
    let profile: Value = sqlx::query_scalar("SELECT profile FROM identities WHERE id=$1")
        .bind(&a.identity)
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    let nonce = "b8".repeat(32);
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
    let mut f = Oracle {
        test: &test,
        steps: vec![],
    };
    let path = format!("/v1/transfer/{id}/0");
    let delete = format!("/v1/transfer/{id}");
    let empty = json!({"origin":"https://app.tryzoen.com"});
    f.record("missing", Method::GET, &path, &[], empty.clone(), 404)
        .await;
    f.record(
        "bad-id-before-body",
        Method::PUT,
        "/v1/transfer/BAD/0",
        &[],
        empty.clone(),
        400,
    )
    .await;
    f.record(
        "chunk-bound",
        Method::GET,
        &format!("/v1/transfer/{id}/4096"),
        &[],
        empty.clone(),
        400,
    )
    .await;
    f.record(
        "bad-index",
        Method::GET,
        &format!("/v1/transfer/{id}/nope"),
        &[],
        empty.clone(),
        400,
    )
    .await;
    f.record(
        "index-overflow",
        Method::GET,
        &format!("/v1/transfer/{id}/4294967296"),
        &[],
        empty.clone(),
        400,
    )
    .await;
    f.record(
        "invalid-utf8",
        Method::GET,
        "/v1/transfer/%FF/0",
        &[],
        empty.clone(),
        400,
    )
    .await;
    f.record(
        "empty-before-signature",
        Method::PUT,
        &path,
        &[],
        empty.clone(),
        413,
    )
    .await;
    f.record(
        "max-plus-one-before-signature",
        Method::PUT,
        &path,
        &vec![0; zoen_relay::transfer::MAX_CHUNK + 1],
        empty.clone(),
        413,
    )
    .await;
    f.record(
        "extractor-limit",
        Method::PUT,
        &path,
        &vec![0; zoen_relay::transfer::MAX_CHUNK + 1025],
        empty.clone(),
        413,
    )
    .await;
    f.record(
        "missing-signature",
        Method::PUT,
        &path,
        &sealed,
        empty.clone(),
        401,
    )
    .await;
    f.record(
        "stale",
        Method::PUT,
        &path,
        &sealed,
        signed(&a, "put", &id, "0", &sealed, 0),
        401,
    )
    .await;
    f.record(
        "extreme-timestamp",
        Method::PUT,
        &path,
        &sealed,
        signed(&a, "put", &id, "0", &sealed, i64::MIN),
        401,
    )
    .await;
    let signed_first = signed(&a, "put", &id, "0", &sealed, now());
    f.record(
        "transfer-first",
        Method::PUT,
        &path,
        &sealed,
        signed_first.clone(),
        201,
    )
    .await;
    f.record(
        "resume-duplicate",
        Method::PUT,
        &path,
        &sealed,
        signed_first.clone(),
        200,
    )
    .await;
    f.record(
        "changed-unsigned-body",
        Method::PUT,
        &path,
        b"tampered",
        signed_first.clone(),
        401,
    )
    .await;
    let changed = b"different correctly signed ciphertext";
    f.record(
        "create-only-different-body",
        Method::PUT,
        &path,
        changed,
        signed(&a, "put", &id, "0", changed, now()),
        200,
    )
    .await;
    f.record(
        "immutable-original",
        Method::GET,
        &path,
        &[],
        empty.clone(),
        200,
    )
    .await;
    f.record(
        "canonical-index",
        Method::PUT,
        &format!("/v1/transfer/{id}/%2B0001"),
        &sealed,
        signed(&a, "put", &id, "1", &sealed, now()),
        201,
    )
    .await;
    f.record(
        "other-transfer",
        Method::PUT,
        &format!("/v1/transfer/{other}/0"),
        &sealed,
        signed(&a, "put", &other, "0", &sealed, now()),
        201,
    )
    .await;
    f.record(
        "max",
        Method::PUT,
        &format!("/v1/transfer/{id}/4095"),
        &vec![0; zoen_relay::transfer::MAX_CHUNK],
        signed(
            &a,
            "put",
            &id,
            "4095",
            &vec![0; zoen_relay::transfer::MAX_CHUNK],
            now(),
        ),
        201,
    )
    .await;
    f.record(
        "delete-unsigned",
        Method::DELETE,
        &delete,
        &[],
        empty.clone(),
        401,
    )
    .await;
    f.record(
        "delete",
        Method::DELETE,
        &delete,
        &[],
        signed(&a, "delete", &id, "", &[], now()),
        204,
    )
    .await;
    f.record(
        "deleted-missing",
        Method::GET,
        &path,
        &[],
        empty.clone(),
        404,
    )
    .await;
    f.record(
        "other-retained",
        Method::GET,
        &format!("/v1/transfer/{other}/0"),
        &[],
        empty.clone(),
        200,
    )
    .await;
    f.record(
        "delete-idempotent",
        Method::DELETE,
        &delete,
        &[],
        signed(&a, "delete", &id, "", &[], now()),
        204,
    )
    .await;
    let media_ts = now();
    let media_sha = hex::encode(Sha256::digest(&sealed));
    f.record("media-shares-quota", Method::PUT, &format!("/v1/blobs/{media_sha}"), &sealed,
        json!({"origin":"https://app.tryzoen.com","x-zoen-device":a.device,"x-zoen-ts":media_ts.to_string(),
            "x-zoen-sig":a.key.sign(&blob_put_message(&media_sha,media_ts,&test.state.relay_name))}), 201).await;
    let clock = now();
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(
        db::revoke_device(&mut tx, &a.identity, a.device.as_deref().unwrap())
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    f.steps.push(json!({"label":"unlink","op":"revoke","now_ms":clock.to_string(),"objects_after":inventory(&test).await}));
    f.record(
        "revoked-fresh",
        Method::PUT,
        &path,
        &sealed,
        signed(&a, "put", &id, "0", &sealed, now()),
        403,
    )
    .await;
    f.record(
        "revoked-duplicate",
        Method::PUT,
        &format!("/v1/transfer/{other}/0"),
        &sealed,
        signed(&a, "put", &other, "0", &sealed, now()),
        403,
    )
    .await;
    f.record(
        "revoked-delete",
        Method::DELETE,
        &format!("/v1/transfer/{other}"),
        &[],
        signed(&a, "delete", &other, "", &[], now()),
        403,
    )
    .await;
    f.record(
        "capability-after-revoke",
        Method::GET,
        &format!("/v1/transfer/{other}/0"),
        &[],
        empty,
        200,
    )
    .await;
    let fixture = json!({"schema":1,"scope":"native-transfer-http","relay":test.state.relay_name,"identity":a.identity,"device":a.device,"certificate":a.cert,"profile":profile,"id":id,"other_id":other,"steps":f.steps,
        "control":{"hello_base64":STANDARD.encode(hello),"nonce":nonce,"auth_base64":STANDARD.encode(auth),"unlink_frame_base64":STANDARD.encode(unlink)},
        "decryption":{"key_hex":hex::encode(key),"plain_base64":STANDARD.encode(plain)}});
    if let Ok(path) = std::env::var("ZOEN_TRANSFER_FIXTURE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    }
    test.close().await;
}
