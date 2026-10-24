use super::*;

pub(super) async fn produce(
    test: &TestRelay,
    store: &ControlledStore,
) -> std::result::Result<Value, Box<dyn std::error::Error>> {
    let a = test.register().await?;
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
    let (key, sealed) = event_log::media::seal(plain)?;
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
    // Exercise the real native arithmetic, without a retired host JSON adapter.
    let clock = 9_007_199_254_740_993;
    let burst_head = clock + policy::DEFAULT_INTERVAL_NS * u64::from(policy::DEFAULT_BURST);
    assert_eq!(
        relay_admission::gcra::decide(
            clock,
            None,
            policy::DEFAULT_BURST,
            policy::DEFAULT_INTERVAL_NS,
            policy::DEFAULT_BURST
        ),
        relay_admission::gcra::Decision::Accepted {
            next_ns: burst_head
        }
    );
    assert_eq!(
        relay_admission::gcra::decide(
            clock,
            Some(burst_head),
            1,
            policy::DEFAULT_INTERVAL_NS,
            policy::DEFAULT_BURST
        ),
        relay_admission::gcra::Decision::Rejected {
            wait_ns: policy::DEFAULT_INTERVAL_NS
        }
    );
    Ok(
        json!({"schema":1,"scope":"native-media-http","relay":test.state.relay_name,"identity":a.identity,
        "device":a.device,"certificate":a.cert,"profile":profile,"initial_registered":true,
        "quota":{"interval_ns":policy::DEFAULT_INTERVAL_NS.to_string(),"burst":policy::DEFAULT_BURST},"steps":f.steps,
        "control":{"hello_base64":STANDARD.encode(hello),"nonce":nonce,"auth_base64":STANDARD.encode(auth),"unlink_frame_base64":STANDARD.encode(unlink)},
        "decryption":{"key_hex":hex::encode(key),"plain_base64":STANDARD.encode(plain)},
        "storage_failures":storage_failures}),
    )
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
