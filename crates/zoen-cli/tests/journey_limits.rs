//! S6 journeys: the relay's abuse controls against real clients and an attacker's raw one.
//! Limits are lowered with `ZOEN_LIMITS` so each journey runs in seconds; the buckets, keys
//! and refusals are the production ones.

mod common;
use common::*;
use roda_proto::{retry_hint, ErrorCode, ServerFrame, PROTOCOL_VERSION};

fn refused_by(metrics: &str, scope: &str) -> u64 {
    metrics
        .lines()
        .find(|l| {
            l.starts_with(&format!(
                "zoen_relay_rate_limited_total{{scope=\"{scope}\"}}"
            ))
        })
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// A real person sending faster than their bucket loses nothing: refusals are transient, the
/// outbox keeps the messages, and the client retries until each one is in.
#[tokio::test]
async fn a_fast_sender_is_slowed_down_and_loses_nothing() {
    let w = World::with_env("limits_send", &[("ZOEN_LIMITS", "publish_device=2/s:3")]).await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "1"]);
    for n in 2..=6 {
        w.zoen("ana", &["send", "@bruno", &n.to_string()]);
    }
    let seen = w.zoen("bruno", &["read", "@ana"]);
    let lines: Vec<&str> = seen.lines().collect();
    assert_eq!(
        lines,
        ["Ana: 1", "Ana: 2", "Ana: 3", "Ana: 4", "Ana: 5", "Ana: 6"],
        "{seen}"
    );
    assert!(
        refused_by(&w.metrics(), "publish_device") > 0,
        "the bucket did refuse"
    );
}

/// An attacker's own client flooding publishes gets "slow down" with a usable retry hint,
/// before the relay spends anything on signatures or storage.
#[tokio::test]
async fn a_publish_flood_is_refused_with_a_retry_hint() {
    let w = World::with_env("limits_flood", &[("ZOEN_LIMITS", "publish_device=5/s:5")]).await;
    let mut m = RawClient::connect(&w.relay_url(), "mallory").await;
    let mut answers = Vec::new();
    for i in 0..12 {
        answers.push(m.publish_message("sp_nowhere", &format!("spam {i}")).await);
    }
    let slowed: Vec<&String> = answers
        .iter()
        .filter(|a| a.starts_with("slow down"))
        .collect();
    assert!(slowed.len() >= 6, "{answers:?}");
    let hint = retry_hint(slowed[0]).expect("a retry hint");
    assert!(hint.as_millis() > 0 && hint.as_secs() < 2, "{hint:?}");
}

/// Accounts per address, handle lookups per account, and handshakes per address.
#[tokio::test]
async fn registrations_lookups_and_handshakes_are_capped() {
    let w = World::with_env(
        "limits_caps",
        &[(
            "ZOEN_LIMITS",
            "register_ip=3/h:3,lookup_account=3/m:3,connect_ip=12/m:12",
        )],
    )
    .await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let (_, third) = RawClient::connect_registering(&w.relay_url(), "mallory1").await;
    assert!(third.is_ok(), "{third:?}");
    let (_, fourth) = RawClient::connect_registering(&w.relay_url(), "mallory2").await;
    let err = fourth.expect_err("a fourth account from one address");
    assert!(err.starts_with("slow down"), "{err}");

    let tries: Vec<Result<String, String>> = (0..5)
        .map(|_| w.try_zoen("ana", &["people", "bru"]))
        .collect();
    assert!(
        tries[0].as_ref().is_ok_and(|o| o.contains("@bruno")),
        "{tries:?}"
    );
    assert!(
        tries
            .iter()
            .any(|t| t.as_ref().is_err_and(|e| e.contains("slow down"))),
        "{tries:?}"
    );

    let mut refused = None;
    for _ in 0..20 {
        if let ServerFrame::Error { code, message } =
            hello_only(&w.relay_url(), PROTOCOL_VERSION).await
        {
            if code == ErrorCode::RateLimited {
                refused = Some(message);
                break;
            }
        }
    }
    let message = refused.expect("handshakes from one address get capped");
    assert!(retry_hint(&message).is_some(), "{message}");
    let m = w.metrics();
    for scope in ["register_ip", "lookup_account", "connect_ip"] {
        assert!(refused_by(&m, scope) > 0, "{scope}:\n{m}");
    }
}
