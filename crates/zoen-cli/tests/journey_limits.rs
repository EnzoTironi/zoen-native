//! S6 journeys: the relay's abuse controls against real clients and an attacker's raw one.
//! Limits are lowered with `ZOEN_LIMITS` so each journey runs in seconds; the buckets, keys
//! and refusals are the production ones.

mod common;
use common::*;
use protocol::{retry_hint, ErrorCode, ServerFrame, PROTOCOL_VERSION};
use std::{io::BufRead, time::Duration};

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
    let mut w = World::new("limits_send").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "1"]);
    // Queue a burst so command startup and catch-up cannot refill the bucket between sends.
    for n in 2..=6 {
        w.zoen("ana", &["send", "@bruno", &n.to_string(), "--offline"]);
    }
    // Inject a real device-bucket refusal after setup, rather than depending on
    // a slow CI runner sending several confirmed messages inside half a second.
    // Its <=12s retry hint still fits the original 30s consumer deadline.
    w.stop_relay();
    w.configure_relay_env("ZOEN_LIMITS", Some("publish_device=5/m:1"));
    w.start_relay();
    let mut sync = w.spawn_zoen_env(
        "ana",
        &["sync", "--timeout", "30000"],
        &[("ZOEN_NET_DEBUG", "1")],
    );
    let stderr = sync.stderr.take().expect("consumer diagnostic pipe");
    let (notice, notices) = std::sync::mpsc::sync_channel(1);
    let diagnostic = std::thread::spawn(move || {
        let mut output = String::new();
        for line in std::io::BufReader::new(stderr).lines() {
            let line = line.expect("read synthetic consumer diagnostic");
            if line.contains("publication ") && line.contains(" refused: ") {
                if let Some(hint) = retry_hint(&line) {
                    let _ = notice.try_send(hint);
                }
            }
            output.push_str(&line);
            output.push('\n');
        }
        output
    });
    let refused = (|| {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Ok(hint) = notices.try_recv() {
                if hint > Duration::from_secs(12) {
                    return Err(format!("retry hint exceeds fixture bound: {hint:?}"));
                }
                let count = refused_by(&w.metrics(), "publish_device");
                if count == 0 {
                    return Err("consumer refusal lacks actual production metric".into());
                }
                return Ok(count);
            }
            if sync
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
                || std::time::Instant::now() >= deadline
            {
                return Err("real CLI did not reach the production bucket refusal".to_string());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    })();
    // Only this fixture's relay changes; the same CLI and its signed pending
    // intents must recover after the observed refusal and reconnect.
    w.stop_relay();
    w.configure_relay_env("ZOEN_LIMITS", None);
    let recovered = w.try_start_relay();
    if refused.is_err() || recovered.is_err() {
        let _ = sync.kill();
    }
    let output = sync
        .wait_with_output()
        .expect("reap original sync consumer");
    let diagnostic = diagnostic.join().expect("join consumer diagnostic reader");
    assert!(refused.is_ok(), "{refused:?}");
    assert!(recovered.is_ok(), "{recovered:?}");
    assert!(output.status.success(), "{}", diagnostic);
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("pending=0"), "{s}");
    let seen = w.zoen("bruno", &["read", "@ana"]);
    let lines: Vec<&str> = seen.lines().collect();
    assert_eq!(
        lines,
        ["Ana: 1", "Ana: 2", "Ana: 3", "Ana: 4", "Ana: 5", "Ana: 6"],
        "{seen}"
    );
    assert!(
        refused.expect("observed real refusal") > 0,
        "the bucket did refuse"
    );
}

/// An attacker's own client flooding publishes gets "slow down" with a usable retry hint,
/// before the relay spends anything on signatures or storage.
#[tokio::test]
async fn a_publish_flood_is_refused_with_a_retry_hint(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // A refill slow enough that no token comes back during the burst, however slow the
    // machine: exactly the first five publishes reach the space check, the rest are refused.
    let w = World::with_env("limits_flood", &[("ZOEN_LIMITS", "publish_device=5/m:5")]).await;
    let mut m = RawClient::connect(&w.relay_url(), "mallory").await?;
    let mut answers = Vec::new();
    for i in 0..12 {
        answers.push(
            m.publish_message("sp_nowhere", &format!("spam {i}"))
                .await?,
        );
    }
    let slowed: Vec<&String> = answers
        .iter()
        .filter(|a| a.starts_with("slow down"))
        .collect();
    assert!(
        answers[..5].iter().all(|a| a == "unknown space"),
        "{answers:?}"
    );
    assert_eq!(slowed.len(), 7, "{answers:?}");
    // One token per 12 s: the hint must point at the next refill, not now and not later.
    let hint = retry_hint(slowed[0]).expect("a retry hint");
    assert!(hint.as_millis() > 0 && hint.as_secs() <= 12, "{hint:?}");
    Ok(())
}

/// Accounts per address, handle lookups per account, and handshakes per address.
#[tokio::test]
async fn registrations_lookups_and_handshakes_are_capped(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
    let (_, third) = RawClient::connect_registering(&w.relay_url(), "mallory1").await?;
    assert!(third.is_ok(), "{third:?}");
    let (_, fourth) = RawClient::connect_registering(&w.relay_url(), "mallory2").await?;
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
            hello_only(&w.relay_url(), PROTOCOL_VERSION).await?
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
    Ok(())
}
