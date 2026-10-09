//! Growth journey (ADR 0044): acquisition source → server-driven onboarding → experiment
//! arm → exposure → funnel by source and by arm, on a real relay with real `zoen` installs.
//! The remote config changes mid-test through the admin endpoint (no new build), which is
//! also how the test pins each install's arm.
//!
//!   ZOEN_TEST_PG=... cargo test -p zoen-cli --test journey_growth

mod common;
use common::*;
use std::io::{Read, Write};

const TOKEN: &str = "journey-admin-token-0123456789abcdef";

fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut extra = String::new();
    for (k, v) in headers {
        extra.push_str(&format!("{k}: {v}\r\n"));
    }
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out[9..12].parse().unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let body = if head.to_lowercase().contains("transfer-encoding: chunked") {
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
    (status, head.to_string(), body)
}

fn auth() -> String {
    format!("Bearer {TOKEN}")
}

/// The built-in config with the onboarding experiment split `control`/`direct` as given,
/// and no holdout.
fn config(control: u32, direct: u32) -> String {
    let mut c: serde_json::Value =
        serde_json::from_str(roda_proto::experiments::DEFAULT_CONFIG).unwrap();
    c["holdout"] = serde_json::Value::Null;
    c["flags"]["onboarding_friend_v1"]["variants"][0]["weight"] = control.into();
    c["flags"]["onboarding_friend_v1"]["variants"][1]["weight"] = direct.into();
    c.to_string()
}

fn put_config(w: &World, body: &str) -> u16 {
    let a = auth();
    http(
        w.port,
        "PUT",
        "/admin/config",
        &[("Authorization", &a), ("Content-Type", "application/json")],
        body,
    )
    .0
}

fn plan(w: &World, who: &str) -> std::collections::BTreeMap<String, String> {
    let relay = w.relay_url();
    w.zoen(who, &["onboarding", "--relay", &relay])
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[tokio::test]
async fn source_picks_the_onboarding_and_metrics_split_by_source_and_arm() {
    let w = World::with_env(
        "growth",
        &[
            ("ZOEN_ADMIN_TOKEN", TOKEN),
            ("ZOEN_METRICS_TEST_HANDLES", "qa_"),
            // HTTP reports must use the authenticated challenge name, not the URL host.
            ("ZOEN_RELAY_NAME", "growth-canonical-relay"),
            ("RUST_LOG", "debug,sqlx=info"),
        ],
    )
    .await;
    let a = auth();

    // Remote config: public, cacheable; changing it needs the admin token and a valid document.
    let (status, head, body) = http(w.port, "GET", "/v1/config", &[], "");
    assert_eq!(status, 200);
    assert!(body.contains("onboarding_friend_v1"), "{body}");
    let etag = head
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .starts_with("etag:")
                .then(|| l[5..].trim().to_string())
        })
        .unwrap();
    assert_eq!(
        http(w.port, "GET", "/v1/config", &[("If-None-Match", &etag)], "").0,
        304
    );
    assert_eq!(
        http(w.port, "PUT", "/admin/config", &[], &config(100, 0)).0,
        401
    );
    assert_eq!(
        put_config(
            &w,
            r#"{"version":1,"flags":{"x":{"salt":"s","variants":[]}}}"#
        ),
        422
    );

    w.init("ana", "Ana");

    // Everyone Ana invites lands in control for now.
    assert_eq!(put_config(&w, &config(100, 0)), 200);
    w.zoen("bia", &["open", "https://tryzoen.com/@ana"]);
    let p = plan(&w, "bia");
    assert_eq!(p["flow"], "default", "{p:?}");
    assert_eq!(p["landing"], "home");
    assert_eq!(p["experiment"], "onboarding_friend_v1:control");
    w.init("bia", "Bia");
    assert_eq!(
        w.zoen("bia", &["growth-sync"]).trim(),
        "config=1 reported=true"
    );

    // Enzo switches the arm remotely: the next friend invite goes straight to the chat.
    assert_eq!(put_config(&w, &config(0, 100)), 200);
    w.zoen("caio", &["open", "zoen://friend/ana?c=ig_story"]);
    let p = plan(&w, "caio");
    assert_eq!(p["flow"], "friend_invite", "{p:?}");
    assert_eq!(p["steps"], "hello,profile,done");
    assert_eq!(p["landing"], "chat_with_inviter");
    assert_eq!(p["target"], "ana");
    assert_eq!(p["experiment"], "onboarding_friend_v1:direct");
    assert_eq!(
        p["copy.onboarding.hello.title@pt"],
        "@ana te chamou pro Zoen."
    );
    w.init("caio", "Caio");
    // The landing: the chat with Ana opens and Caio says hi.
    w.zoen("caio", &["dm", "@ana", "oi Ana! cheguei"]);
    assert!(w
        .zoen("ana", &["read", "@caio"])
        .contains("oi Ana! cheguei"));
    assert_eq!(
        w.zoen("caio", &["growth-sync", "--health", "10", "0"])
            .trim(),
        "config=2 reported=true"
    );
    // Nothing left to report: the source goes once, exposures once.
    assert_eq!(
        w.zoen("caio", &["growth-sync"]).trim(),
        "config=2 reported=false"
    );
    // A later link doesn't overwrite the first touch.
    assert!(w
        .zoen("caio", &["open", "zoen://join/ZZZZZZZZZZ"])
        .contains("source=friend"));

    // A Space link goes into the Space.
    w.zoen("ana", &["group", "Trilha", "@caio"]);
    let code = w.zoen("ana", &["invite", "Trilha"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    w.zoen("duda", &["open", &format!("zoen://join/{code}")]);
    let p = plan(&w, "duda");
    assert_eq!(
        (
            p["flow"].as_str(),
            p["landing"].as_str(),
            p["target"].as_str()
        ),
        ("space_link", "space", code.as_str())
    );
    assert_eq!(p["experiment"], "");
    w.init("duda", "Duda");
    w.zoen("duda", &["join", &format!("zoen://join/{code}")]);
    w.zoen("duda", &["growth-sync"]);

    // An ad campaign gets its own intro; organic gets the default.
    w.zoen(
        "fabi",
        &["open", "https://tryzoen.com/?utm_campaign=ads_concurso"],
    );
    assert_eq!(plan(&w, "fabi")["flow"], "campaign_study");
    w.init("fabi", "Fabi");
    w.zoen("fabi", &["growth-sync"]);
    assert_eq!(plan(&w, "edu")["flow"], "default");
    w.init("edu", "Edu");
    w.zoen("edu", &["growth-sync"]);

    let (status, _, body) = http(
        w.port,
        "GET",
        "/admin/metrics",
        &[("Authorization", &a)],
        "",
    );
    assert_eq!(status, 200, "{body}");
    let r: serde_json::Value = serde_json::from_str(&body).unwrap();
    let src = &r["acquisition"]["by_source"];
    assert_eq!(src["friend"]["signups"], 1, "Bia: {r:#}");
    assert_eq!(src["friend:ig_story"]["signups"], 1, "Caio: {r:#}");
    assert_eq!(
        src["friend:ig_story"]["sent_first_message_pct"], 100.0,
        "{r:#}"
    );
    assert_eq!(src["space"]["signups"], 1, "{r:#}");
    assert_eq!(src["campaign:ads_concurso"]["signups"], 1, "{r:#}");
    assert_eq!(src["organic"]["signups"], 1, "Edu: {r:#}");
    assert_eq!(src["unknown"]["signups"], 1, "Ana never reported: {r:#}");
    let arms = &r["acquisition"]["by_onboarding_arm"];
    assert_eq!(arms["onboarding_friend_v1:control"]["signups"], 1, "{r:#}");
    assert_eq!(arms["onboarding_friend_v1:direct"]["signups"], 1, "{r:#}");
    assert_eq!(
        arms["onboarding_friend_v1:direct"]["activation_24h_pct"], 100.0,
        "{r:#}"
    );
    assert_eq!(
        arms["onboarding_friend_v1:control"]["activation_24h_pct"], 0.0,
        "{r:#}"
    );
    let exp = &r["experiments"]["onboarding_friend_v1"];
    assert_eq!(exp["control"], "control");
    assert_eq!(exp["arms"]["control"]["units"], 1, "{r:#}");
    assert_eq!(exp["arms"]["direct"]["units"], 1, "{r:#}");
    assert_eq!(exp["arms"]["direct"]["health_sessions"], 10, "{r:#}");
    assert_eq!(
        exp["arms"]["direct"]["crash_free_sessions_pct"], 100.0,
        "{r:#}"
    );
    assert_eq!(
        exp["decision"], "keep running",
        "two units decide nothing: {r:#}"
    );

    // Only kinds and campaign ids were stored; no friend, code or id in reports or logs.
    let sources: Vec<String> = {
        let mut c = sqlx::PgConnection::connect(&w.db_url).await.unwrap();
        use sqlx::Connection;
        sqlx::query_scalar(
            "SELECT DISTINCT source FROM metrics_accounts WHERE source IS NOT NULL ORDER BY 1",
        )
        .fetch_all(&mut c)
        .await
        .unwrap()
    };
    assert_eq!(sources, ["campaign", "friend", "organic", "space"]);
    let log = w.relay_log_text();
    for h in ["ana", "bia", "caio", "duda", "edu", "fabi"] {
        let id = w.id_of(h).await;
        assert!(
            !body.contains(&id[..12]) && !body.contains(&format!("@{h}")),
            "report names {h}"
        );
        assert!(
            !log.contains(&id) && !log.contains(&id[..12]),
            "log names {h}"
        );
    }
    assert!(!body.contains(&code), "report carries the invite code");
}
