//! Success metrics journey (ADR 0043): real relay, Postgres, FoundationDB and `zoen`
//! processes. People chat over simulated days (the relay's metrics clock is shifted through
//! `ZOEN_METRICS_CLOCK_FILE`, debug builds only), then the admin endpoint must report the
//! right MAU, messages per active user, activation, retention and invites, while neither
//! the report nor the relay's log ever names an account.
//!
//!   ZOEN_TEST_PG=... cargo test -p zoen-cli --test journey_metrics

mod common;
use common::*;
use std::io::{Read, Write};

const TOKEN: &str = "journey-admin-token-0123456789abcdef";

fn get(port: u16, path: &str, token: Option<&str>) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let auth = token.map_or(String::new(), |t| format!("Authorization: Bearer {t}\r\n"));
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out[9..12].parse().unwrap();
    let body = out.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    // The relay answers with a fixed length or chunked; strip chunk framing if present.
    let body = if out.to_lowercase().contains("transfer-encoding: chunked") {
        let mut rest = body;
        let mut joined = String::new();
        while let Some((len, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            joined.push_str(&tail[..n]);
            rest = &tail[n + 2..];
        }
        joined
    } else {
        body.to_string()
    };
    (status, body)
}

fn report(w: &World, everyone: bool) -> serde_json::Value {
    let path = if everyone {
        "/admin/metrics?population=everyone"
    } else {
        "/admin/metrics"
    };
    let (status, body) = get(w.port, path, Some(TOKEN));
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn mau_and_messages_per_user_over_simulated_days() {
    let clock = std::env::temp_dir().join(format!("zoen-metrics-clock-{}", now_ms()));
    std::fs::write(&clock, "0").unwrap();
    let clock_s = clock.to_string_lossy().to_string();
    let w = World::with_env(
        "metrics",
        &[
            ("ZOEN_ADMIN_TOKEN", TOKEN),
            ("ZOEN_METRICS_CLOCK_FILE", &clock_s),
            ("ZOEN_METRICS_TEST_HANDLES", "qa_"),
            ("RUST_LOG", "debug,sqlx=debug"),
        ],
    )
    .await;
    let day = |n: i32| std::fs::write(&clock, n.to_string()).unwrap();

    // Admin surface: nothing without the token.
    assert_eq!(get(w.port, "/admin/metrics", None).0, 401);
    assert_eq!(
        get(w.port, "/admin/metrics", Some("wrong-token-wrong-token")).0,
        401
    );

    // Day 0: four people and a QA account sign up.
    for (h, n) in [
        ("ana", "Ana"),
        ("bruno", "Bruno"),
        ("carla", "Carla"),
        ("davi", "Davi"),
        ("qa_bot", "QA"),
    ] {
        w.init(h, n);
    }
    // Baseline: what setting up a chat costs in application envelopes, measured, not assumed.
    let before = report(&w, false)["headline"]["messages"]["day"]
        .as_i64()
        .unwrap();
    assert_eq!(before, 0, "signing up isn't sending");

    w.zoen("ana", &["group", "Trilha", "@bruno"]);
    let setup = report(&w, true)["headline"]["messages"]["day"]
        .as_i64()
        .unwrap();
    w.zoen("ana", &["send", "Trilha", "bora fazer trilha?"]);
    w.zoen("bruno", &["send", "Trilha", "bora!"]);
    let after = report(&w, true)["headline"]["messages"]["day"]
        .as_i64()
        .unwrap();
    assert_eq!(after - setup, 2, "two messages are two messages");

    // Carla arrives by invite and writes; the QA account writes to Ana.
    let code = w.zoen("ana", &["invite", "Trilha"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    w.zoen("carla", &["join", &format!("zoen://join/{code}")]);
    w.zoen("ana", &["sync"]); // Ana's device commits Carla into the end-to-end group
    w.zoen("carla", &["sync"]);
    w.zoen("carla", &["send", "Trilha", "cheguei"]);
    w.zoen("qa_bot", &["dm", "@ana", "teste automatizado"]);

    let r = report(&w, false);
    let h = &r["headline"];
    // Davi only opened the app (it synced): active, but not a sender (the strict variant).
    assert_eq!(h["dau"], 4, "{r:#}");
    assert_eq!(h["mau"], 4, "{r:#}");
    assert_eq!(h["senders"]["dau"], 3, "Ana, Bruno and Carla wrote: {r:#}");
    assert_eq!(h["messages"]["day"], 3, "{r:#}");
    assert_eq!(r["growth"]["invites_created_30d"], 1, "{r:#}");
    assert_eq!(r["growth"]["invites_accepted_30d"], 1, "{r:#}");
    assert_eq!(
        r["growth"]["signups_today"], 4,
        "QA account not counted: {r:#}"
    );
    // Everyone (QA included) sees one more active account.
    assert_eq!(report(&w, true)["headline"]["mau"], 5);
    let day0_msgs = h["messages"]["day"].as_i64().unwrap();

    // Day 1: Ana writes twice, Bruno only reads.
    day(1);
    w.zoen("ana", &["send", "Trilha", "sábado 7h"]);
    w.zoen("ana", &["send", "Trilha", "levem água"]);
    w.zoen("bruno", &["read", "Trilha"]);
    let r = report(&w, false);
    assert_eq!(r["headline"]["dau"], 2, "Ana sent, Bruno synced: {r:#}");
    assert_eq!(r["headline"]["senders"]["dau"], 1, "{r:#}");
    assert_eq!(r["headline"]["messages"]["day"], 2, "{r:#}");
    assert_eq!(
        r["headline"]["messages_per_active_user"]["day"], 2.0,
        "{r:#}"
    );

    // Day 7: Carla comes back and writes; Day 8: nobody.
    day(7);
    w.zoen("carla", &["send", "Trilha", "foi ótimo!"]);
    day(8);
    let r = report(&w, false);
    let h = &r["headline"];
    assert_eq!(h["mau"], 4, "{r:#}");
    assert_eq!(h["wau"], 1, "only Carla in the last 7 days: {r:#}");
    assert_eq!(h["dau"], 0, "{r:#}");
    let month_msgs = h["messages"]["month"].as_i64().unwrap();
    assert_eq!(month_msgs, day0_msgs + 2 + 1, "{r:#}");
    assert_eq!(h["senders"]["mau"], 3);
    let per_user = h["messages_per_active_user"]["month"].as_f64().unwrap();
    assert!((per_user - month_msgs as f64 / 3.0).abs() < 0.01, "{r:#}");

    // Retention and activation are over the four people who signed up on day 0.
    let ret = &r["retention"];
    assert_eq!(ret["d1"]["den"], 4, "{r:#}");
    assert_eq!(
        ret["d1"]["num"], 2,
        "Ana and Bruno came back on day 1: {r:#}"
    );
    assert_eq!(ret["d7"]["num"], 1, "Carla on day 7: {r:#}");
    assert_eq!(
        r["growth"]["activation_first_message_24h_pct"], 75.0,
        "3 of 4: {r:#}"
    );
    // Reached two people in week 1: Ana (wrote again once Carla was in) and Carla. Bruno only
    // wrote before Carla joined. The sketch is 64 bits, so a collision may hide one (1/64).
    let two = r["growth"]["activation_2_people_week1_pct"]
        .as_f64()
        .unwrap();
    assert!(two == 50.0 || two == 25.0, "{r:#}");
    assert!(
        r["growth"]["invited_time_to_first_message_median_s"].is_number(),
        "{r:#}"
    );
    assert_eq!(
        r["growth"]["viral_k_30d"], 0.25,
        "1 accepted invite / 4 signups: {r:#}"
    );
    // 30 days of series, today last.
    let series = r["series"].as_array().unwrap();
    assert!(series.len() >= 9, "{r:#}");
    assert_eq!(series.last().unwrap()["mau"], 4);
    assert!(r["reliability"]["send_latency_ms_p50"].is_number(), "{r:#}");
    assert!(
        r["engagement"]["spaces_active_avg_members"].is_null(),
        "no Space active on day 8"
    );

    // Nothing in the report or the relay's debug log names anyone.
    let text =
        serde_json::to_string(&r).unwrap() + &serde_json::to_string(&report(&w, true)).unwrap();
    let log = w.relay_log_text();
    for h in ["ana", "bruno", "carla", "davi", "qa_bot"] {
        let id = w.id_of(h).await;
        assert!(
            !text.contains(&id) && !text.contains(&id[..12]),
            "report names {h}"
        );
        assert!(
            !text.contains(&format!("\"{h}\"")) && !text.contains(&format!("@{h}")),
            "report names @{h}"
        );
        assert!(
            !log.contains(&id) && !log.contains(&id[..12]),
            "relay log names {h}"
        );
    }
    // The metrics tables hold pseudonyms only.
    let ana = w.id_of("ana").await;
    assert_eq!(
        w.count(&format!(
            "SELECT count(*) FROM metrics_activity WHERE encode(pid, 'hex') LIKE '%{}%'",
            &ana[..12]
        ))
        .await,
        0
    );
    assert!(w.count("SELECT count(*) FROM metrics_daily").await >= 8);
    let _ = std::fs::remove_file(&clock);
}
