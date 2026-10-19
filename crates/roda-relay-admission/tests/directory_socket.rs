use base64::{engine::general_purpose::STANDARD, Engine};
use roda_proto::{ClientFrame, ErrorCode, Op, ServerFrame};
use roda_relay_admission::directory::socket;
use serde_json::{json, Value};
fn control(value: Value) -> Vec<u8> {
    let value: Value =
        serde_json::from_str(&socket::control_json(&value.to_string()).unwrap()).unwrap();
    STANDARD
        .decode(value["frame_base64"].as_str().unwrap())
        .unwrap()
}
#[test]
fn native_ready_pong_and_refusal_bytes_preserve_full_uint64() {
    let identity = "a".repeat(64);
    assert_eq!(
        control(
            json!({"kind":"ready","session":{"identity":identity,"device":"b".repeat(64),"cert":"cert","registered":true}})
        ),
        ServerFrame::Ready {
            identity,
            registered: true
        }
        .encode()
    );
    for time in [None, Some(1_800_000_000_000i64)] {
        assert_eq!(
            control(json!({"kind":"pong","server_time_ms":time.map(|value|value.to_string())})),
            ServerFrame::Pong {
                server_time_ms: time
            }
            .encode()
        );
    }
    assert_eq!(
        control(json!({"kind":"refused","id":"9007199254740993","reason":"no"})),
        ServerFrame::Res {
            id: 9_007_199_254_740_993,
            result: Err("no".into())
        }
        .encode()
    );
    assert!(socket::control_json(
        &json!({"kind":"refused","id":"18446744073709551616","reason":"no"}).to_string()
    )
    .is_err());
}
#[test]
fn native_error_categories_and_request_failure_envelopes_stay_exact() {
    for (stage, reason, permanent, code) in [
        (
            "hello",
            "device certificate invalid",
            true,
            ErrorCode::Unauthorized,
        ),
        (
            "hello",
            "protocol 0 is too old",
            true,
            ErrorCode::UpgradeRequired,
        ),
        ("auth", "expected auth", true, ErrorCode::Other),
        (
            "auth",
            "login signature invalid",
            true,
            ErrorCode::Unauthorized,
        ),
        (
            "ready",
            "database unavailable",
            false,
            ErrorCode::Unavailable,
        ),
    ] {
        assert_eq!(
            control(json!({"kind":"error","stage":stage,"reason":reason,"permanent":permanent})),
            ServerFrame::error(code, reason).encode()
        );
    }
}
#[test]
fn actual_client_codec_classifies_without_js_protobuf_or_numeric_ids() {
    assert_eq!(
        socket::incoming(&ClientFrame::Ping.encode()).unwrap(),
        json!({"kind":"ping"})
    );
    let bytes = ClientFrame::Req {
        id: u64::MAX,
        op: Op::ClaimKeyPackages {
            ids: vec![],
            operation_id: None,
        },
    }
    .encode();
    assert_eq!(
        socket::incoming(&bytes).unwrap(),
        json!({"kind":"request","id":u64::MAX.to_string()})
    );
    assert_eq!(
        socket::incoming(
            &ClientFrame::Sync {
                cursors: vec![],
                all: true
            }
            .encode()
        )
        .unwrap(),
        json!({"kind":"unsupported"})
    );
    for bytes in [vec![], vec![0], vec![7; 1024 * 1024 + 1]] {
        assert!(socket::incoming(&bytes).is_err());
    }
}
#[test]
fn private_delivery_preserves_original_bytes_and_cannot_inject_handshake_frames() {
    let bytes = ServerFrame::Res {
        id: u64::MAX,
        result: Err("original".into()),
    }
    .encode();
    let result: Value = serde_json::from_str(&socket::delivery_json(&bytes).unwrap()).unwrap();
    assert_eq!(
        STANDARD
            .decode(result["frame_base64"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert!(socket::delivery_json(
        &ServerFrame::Ready {
            identity: "a".repeat(64),
            registered: true
        }
        .encode()
    )
    .is_err());
    assert!(socket::delivery_json(&[]).is_err());
}
