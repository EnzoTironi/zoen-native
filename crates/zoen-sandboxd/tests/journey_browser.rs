//! Journey (ADR 0028 §7, P2): Ana asks her agent to check an order at a web shop.
//!
//! The agent gets a browser microVM: headless Chromium, warm from a snapshot, driven over a
//! CDP pipe that never leaves the VM. It can open, read, click and type, and nothing else.
//! Every request goes through the egress proxy, signed as Zoen's agent (Web Bot Auth), so the
//! shop lets it in. When the shop asks for a password and a 2FA code, the agent can't type
//! them: Ana takes over from her phone. She watches a live view whose frames are sealed to
//! her phone's key, and her keystrokes are sealed to the VM; the model gets nothing while she
//! has the browser, and afterwards never sees what she typed. Measured along the way: memory
//! per browser VM, warm-start time, and how many browsers fit on a host.

mod common;

use common::*;
use roda_types::AgentRequest;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use zoen_agentd::{BrowserCall, BrowserTools, ExecRequest, SandboxProvider, SandboxSpec, Tier};
use zoen_egress::webbotauth::verify_request;
use zoen_egress::{
    ApprovalSink, Egress, EgressConfig, EgressRule, Resolver, SecretSource, SignedAgent,
};
use zoen_liveview::{DeviceKey, Direction, InputEvent};
use zoen_sandboxd::{FirecrackerProvider, Shape};

const PASSWORD: &str = "correct-horse-battery-9";
const OTP: &str = "481516";
const USER: &str = "ana@example.test";

struct NoSecrets;
impl SecretSource for NoSecrets {
    fn secret(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

#[derive(Default)]
struct Cards(Mutex<Vec<AgentRequest>>);
impl ApprovalSink for Cards {
    fn opened(&self, _lease: &str, r: &AgentRequest) {
        self.0.lock().unwrap().push(r.clone());
    }
}

/// What the shop saw: whether each request was signed by Zoen, and whether the login worked.
#[derive(Default)]
struct ShopLog {
    signed: usize,
    unsigned: usize,
    logged_in: bool,
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=utf-8><title>{title}</title></head>\
         <body style=\"font-family:sans-serif\">{body}</body></html>"
    )
}

fn form_value(body: &str, key: &str) -> String {
    body.split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.replace('+', " ").replace("%40", "@"))
        .unwrap_or_default()
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    mut s: S,
    agent: Arc<SignedAgent>,
    log: Arc<Mutex<ShopLog>>,
) {
    let mut buf = vec![];
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        let n = s.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let h = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let len: usize = h("content-length").parse().unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
    let verified = verify_request(&headers, &h("host"), &agent.jwks(), now_s()).is_ok();
    {
        let mut l = log.lock().unwrap();
        if verified {
            l.signed += 1;
        } else {
            l.unsigned += 1;
        }
    }
    let (method, path) = {
        let mut p = first.split(' ');
        (
            p.next().unwrap_or("").to_string(),
            p.next().unwrap_or("/").to_string(),
        )
    };
    let mut set_cookie = String::new();
    let (status, html) = if !verified {
        // Where a real site would show a CAPTCHA.
        (
            "403 Forbidden",
            page("Challenge", "<h1>Are you a robot?</h1>"),
        )
    } else if method == "GET" && path == "/" {
        (
            "200 OK",
            page(
                "Zoen Shop",
                "<h1>Welcome to the shop</h1><p>Signed agent: verified</p>\
                 <a href=\"/docs\">Shipping info</a> <a href=\"/login\">Sign in</a>\
                 <form action=\"/search\"><input name=\"q\" placeholder=\"Search\">\
                 <button>Go</button></form>",
            ),
        )
    } else if path == "/docs" {
        ("200 OK", page("Shipping", "<p>Shipping takes 3 days.</p>"))
    } else if let Some(q) = path.strip_prefix("/search?q=") {
        let q = q.replace('+', " ");
        (
            "200 OK",
            page("Search", &format!("<p>Results for {q}: Blue mug</p>")),
        )
    } else if method == "GET" && path == "/login" {
        (
            "200 OK",
            page(
                "Sign in",
                "<form method=\"post\" action=\"/login\">\
                 <label>Email <input id=\"user\" name=\"user\"></label>\
                 <label>Password <input id=\"password\" name=\"password\" type=\"password\"></label>\
                 <label>Code <input id=\"otp\" name=\"code\" autocomplete=\"one-time-code\"></label>\
                 <button id=\"go\">Sign in</button></form>",
            ),
        )
    } else if method == "POST" && path == "/login" {
        let ok = form_value(&body, "user") == USER
            && form_value(&body, "password") == PASSWORD
            && form_value(&body, "code") == OTP;
        if ok {
            log.lock().unwrap().logged_in = true;
            set_cookie = "Set-Cookie: session=s1; Path=/; Secure; HttpOnly\r\n".into();
            // A careless site that echoes the password back.
            (
                "200 OK",
                page(
                    "Account",
                    &format!(
                        "<p>Signed in as {USER}.</p><p>debug: password {PASSWORD}</p>\
                         <a href=\"/account\">Orders</a>"
                    ),
                ),
            )
        } else {
            ("200 OK", page("Sign in", "<p>Wrong password or code.</p>"))
        }
    } else if path == "/account" {
        if h("cookie").contains("session=s1") {
            (
                "200 OK",
                page("Orders", "<p>Order 1042: 2 blue mugs, shipped.</p>"),
            )
        } else {
            ("200 OK", page("Orders", "<p>Please sign in.</p>"))
        }
    } else {
        ("404 Not Found", page("Not found", ""))
    };
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = s.write_all(resp.as_bytes()).await;
    let _ = s.shutdown().await;
}

/// `shop.example.test` over HTTPS with its own CA (trusted by the egress proxy upstream).
async fn shop(agent: Arc<SignedAgent>, log: Arc<Mutex<ShopLog>>) -> (u16, String) {
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_cert = ca.self_signed(&ca_key).unwrap();
    let issuer = rcgen::Issuer::new(ca, ca_key);
    let key = rcgen::KeyPair::generate().unwrap();
    let leaf = rcgen::CertificateParams::new(vec!["shop.example.test".to_string()])
        .unwrap()
        .signed_by(&key, &issuer)
        .unwrap();
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(leaf.der().to_vec())],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    )
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((c, _)) = l.accept().await {
            let (a, agent, log) = (acceptor.clone(), agent.clone(), log.clone());
            tokio::spawn(async move {
                if let Ok(t) = a.accept(c).await {
                    serve(t, agent, log).await;
                }
            });
        }
    });
    (port, ca_cert.pem())
}

/// The element number `Read` gave the line containing `needle`.
fn element(read: &str, needle: &str) -> String {
    read.lines()
        .find(|l| l.starts_with('[') && l.contains(needle))
        .and_then(|l| l[1..].split(']').next())
        .unwrap_or_else(|| panic!("no element {needle:?} in:\n{read}"))
        .to_string()
}

fn kib(v: &str) -> u64 {
    v.split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ana_s_agent_shops_and_ana_signs_in_herself() {
    let Some(mut cfg) = config("journey-browser") else {
        return;
    };
    if cfg.browser_rootfs.is_none() {
        assert!(
            std::env::var("ZOEN_REQUIRE_FIRECRACKER").as_deref() != Ok("1"),
            "ZOEN_REQUIRE_FIRECRACKER=1 but there's no browser image (scripts/firecracker.sh browser-image)"
        );
        eprintln!("skipping: no browser image");
        return;
    }
    cfg.pool_size = 0;
    cfg.browser_pool_size = 1;
    let agent = Arc::new(SignedAgent::generate("https://agents.zoen.test"));
    let shop_log = Arc::new(Mutex::new(ShopLog::default()));
    let (port, shop_ca) = shop(agent.clone(), shop_log.clone()).await;
    let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let cards = Arc::new(Cards::default());
    let egress = Egress::new(
        EgressConfig {
            allow_loopback_upstreams: true,
            extra_upstream_roots_pem: vec![shop_ca],
            signed_agent: Some(agent.clone()),
            ..Default::default()
        },
        Resolver::Static(HashMap::from([
            ("shop.example.test".to_string(), vec![lo]),
            ("tracker.example.test".to_string(), vec![lo]),
        ])),
        Arc::new(NoSecrets),
        cards.clone(),
    );
    let p = FirecrackerProvider::new(cfg, Some(egress.clone()));
    let mut rule = EgressRule::host("shop.example.test");
    rule.ports = vec![port];
    let spec = SandboxSpec {
        tier: Tier::Browser,
        tool: "web-shopper".into(),
        template: "browser".into(),
        vcpu: 2,
        mem_mib: 1024,
        disk_mib: 256,
        max_secs: 300,
        egress: vec![rule],
        secrets: vec![],
        agent: Some("id_ana_agent".into()),
    };
    let base = format!("https://shop.example.test:{port}");

    // The template: one cold boot, Chromium started inside, one snapshot.
    let built = p.warm(Shape::of(&spec)).await.unwrap().unwrap();
    println!(
        "browser template (cold boot + Chromium + snapshot): {}",
        ms(built)
    );

    // 1. Warm start: a lease from the pool, then the first page.
    let t0 = Instant::now();
    let lease = p.acquire(&spec, "id_ana").await.unwrap();
    let acquired = t0.elapsed();
    assert!(
        p.last_acquire().from_pool,
        "the browser comes from the warm pool"
    );
    let r = p
        .browser(
            &lease,
            BrowserCall::Open {
                url: format!("{base}/"),
            },
        )
        .await
        .unwrap();
    let first_page = t0.elapsed();
    assert!(r.contains("Zoen Shop"), "{r}");
    println!(
        "warm start: lease {} , first page loaded {}",
        ms(acquired),
        ms(first_page)
    );

    // 2. The shop saw a signed agent (Web Bot Auth through the egress proxy, over HTTPS
    //    Chromium trusts via the node root) and let it in.
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("Welcome to the shop"), "{read}");
    assert!(read.contains("Signed agent: verified"), "{read}");

    // 3. Click and type, by the numbers Read gave.
    let n = element(&read, "link \"Shipping info\"");
    let t = Instant::now();
    p.browser(&lease, BrowserCall::Click { target: n })
        .await
        .unwrap();
    let click_ms = t.elapsed();
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("Shipping takes 3 days"), "{read}");
    p.browser(
        &lease,
        BrowserCall::Open {
            url: format!("{base}/"),
        },
    )
    .await
    .unwrap();
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    let search = element(&read, "\"Search\"");
    p.browser(
        &lease,
        BrowserCall::Type {
            target: search,
            text: "blue mug".into(),
            submit: true,
        },
    )
    .await
    .unwrap();
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("Results for blue mug: Blue mug"), "{read}");
    println!("click to next page loaded: {}", ms(click_ms));

    // 4. Off the list: refused, and Ana gets a card.
    let err = p
        .browser(
            &lease,
            BrowserCall::Open {
                url: format!("https://tracker.example.test:{port}/"),
            },
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("ERR_"), "{err}");
    assert!(cards
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|c| c.audience == "tracker.example.test"));

    // 5. Memory. A second browser from the same template (the pool refilled meanwhile),
    //    on the same pages: the template's memory file is shared page cache, so what each
    //    extra browser really costs is what it wrote since its restore.
    let t = Instant::now();
    p.browser(
        &lease,
        BrowserCall::Open {
            url: format!("{base}/docs"),
        },
    )
    .await
    .unwrap();
    let steady_open = t.elapsed();
    let second = p.acquire(&spec, "id_ana").await.unwrap();
    for path in ["/", "/docs", "/search?q=blue+mug"] {
        p.browser(
            &second,
            BrowserCall::Open {
                url: format!("{base}{path}"),
            },
        )
        .await
        .unwrap();
    }
    let host_kib = std::fs::read_to_string("/proc/meminfo")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("MemTotal:"))
        .map(kib)
        .unwrap();
    let mut vms = vec![];
    for l in [&lease, &second] {
        let cg = p.cgroup_of(l).unwrap();
        let pid = common::read(&cg.join("cgroup.procs"));
        let rollup = std::fs::read_to_string(format!(
            "/proc/{}/smaps_rollup",
            pid.lines().next().unwrap_or("0")
        ))
        .unwrap_or_default();
        let field = |name: &str| {
            rollup
                .lines()
                .find(|l| l.starts_with(name))
                .map(kib)
                .unwrap_or(0)
        };
        let guest = p
            .exec(
                l,
                ExecRequest::sh(
                    "awk '/MemTotal/{t=$2}/MemAvailable/{a=$2}END{print t-a}' /proc/meminfo; \
                     for p in /proc/[0-9]*; do grep -q '^Name:.*chrom' $p/status 2>/dev/null && \
                     awk '/^Pss:/{print $2}' $p/smaps_rollup; done | awk '{s+=$1}END{print s+0}'",
                ),
            )
            .await
            .unwrap()
            .stdout_str();
        let mut g = guest.lines().map(|v| v.trim().parse::<u64>().unwrap_or(0));
        vms.push((
            read_u64(&cg.join("memory.peak")),
            field("Rss:"),
            field("Pss:"),
            field("Anonymous:"),
            g.next().unwrap_or(0),
            g.next().unwrap_or(0),
        ));
    }
    for (i, (peak, rss, pss, anon, guest_used, chrome_pss)) in vms.iter().enumerate() {
        println!(
            "browser VM {}: cgroup peak {} MiB; VMM rss {} MiB, pss {} MiB, private (written since restore) {} MiB; guest in use {} MiB, of which Chromium {} MiB",
            i + 1,
            peak >> 20,
            rss >> 10,
            pss >> 10,
            anon >> 10,
            guest_used >> 10,
            chrome_pss >> 10
        );
        assert!(
            *peak <= (1024 + 64) << 20,
            "the cgroup holds the VM to its shape"
        );
    }
    let shared_mib = (vms[0].1.saturating_sub(vms[0].3)) >> 10;
    let marginal_mib = (((vms[0].3 + vms[1].3) / 2) >> 10).max(1);
    let host_mib = host_kib >> 10;
    let fit = |host: u64| host.saturating_sub(2048 + shared_mib) / marginal_mib;
    println!(
        "browsers per host: template pages shared {shared_mib} MiB, each browser adds ~{marginal_mib} MiB on these pages -> {} on this {host_mib} MiB host, {} per 64 GiB (2 GiB kept for the host)",
        fit(host_mib),
        fit(64 * 1024)
    );
    println!("steady-state page open: {}", ms(steady_open));
    p.release(second).await.unwrap();

    // 6. The sign-in page. The agent may not type a password or a code.
    p.browser(
        &lease,
        BrowserCall::Open {
            url: format!("{base}/login"),
        },
    )
    .await
    .unwrap();
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("(owner only)"), "{read}");
    let err = p
        .browser(
            &lease,
            BrowserCall::Type {
                target: "#password".into(),
                text: "guess".into(),
                submit: false,
            },
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("FIELD_NEEDS_OWNER"), "{err}");

    // 7. Ana opens the live view on her phone. Frames reach the host sealed; only her
    //    phone's key opens them.
    let phone = DeviceKey::generate();
    let mut live = p.live_start(&lease, phone.public()).await.unwrap();
    let keys = phone.accept(&live.offer);
    let mut frames_in = keys.opener(Direction::Frames);
    let t = Instant::now();
    let first = tokio::time::timeout(Duration::from_secs(20), live.frames.recv())
        .await
        .expect("a live frame within 20 s")
        .unwrap();
    let first_frame = t.elapsed();
    assert_ne!(&first[8..10], b"\xff\xd8", "the host relays ciphertext");
    let jpeg = frames_in.open(&first).unwrap();
    assert_eq!(&jpeg[..2], b"\xff\xd8", "Ana's phone gets a JPEG");
    let mut relayed: Vec<Vec<u8>> = vec![first.clone()];
    println!(
        "live view: first frame {} after start, {} KiB sealed",
        ms(first_frame),
        first.len() / 1024
    );

    // 8. She takes over. The agent gets nothing while she has the browser.
    p.takeover_begin(&lease).await.unwrap();
    for call in [
        BrowserCall::Read,
        BrowserCall::Open {
            url: format!("{base}/"),
        },
        BrowserCall::Click {
            target: "#go".into(),
        },
    ] {
        let e = p.browser(&lease, call).await.unwrap_err().to_string();
        assert!(e.contains("TAKEOVER_IN_PROGRESS"), "{e}");
    }
    let e = p
        .exec(&lease, ExecRequest::sh("cat /proc/meminfo"))
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("TAKEOVER_IN_PROGRESS"), "{e}");

    // Her phone types; each event is sealed to the VM.
    let mut up = keys.sealer(Direction::Input);
    let mut sent: Vec<Vec<u8>> = vec![];
    let events = [
        InputEvent::ClickSelector {
            selector: "#user".into(),
        },
        InputEvent::Text { text: USER.into() },
        InputEvent::ClickSelector {
            selector: "#password".into(),
        },
        InputEvent::Text {
            text: PASSWORD.into(),
        },
        InputEvent::ClickSelector {
            selector: "#otp".into(),
        },
        InputEvent::Text { text: OTP.into() },
        InputEvent::Key {
            key: "Enter".into(),
        },
    ];
    for ev in &events {
        let sealed = up.seal(&serde_json::to_vec(ev).unwrap());
        assert!(!p.takeover_input(&lease, &sealed).await.unwrap());
        sent.push(sealed);
    }
    // A replayed keystroke, and one the host made up, are refused.
    let e = p.takeover_input(&lease, &sent[3]).await.unwrap_err();
    assert!(e.to_string().contains("replay"), "{e}");
    let impostor = DeviceKey::generate().accept(&live.offer);
    let mut forged = impostor.sealer(Direction::Input);
    for _ in 0..20 {
        forged.seal(b"{}");
    }
    let done_forged = forged.seal(&serde_json::to_vec(&InputEvent::Done).unwrap());
    assert!(p.takeover_input(&lease, &done_forged).await.is_err());
    // ...so the takeover is still on.
    assert_eq!(p.browser_status(&lease).await.unwrap()["takeover"], true);
    assert!(
        shop_log.lock().unwrap().logged_in,
        "Ana's sign-in reached the shop"
    );

    // Frames kept coming while she typed.
    while let Ok(Some(f)) =
        tokio::time::timeout(Duration::from_millis(500), live.frames.recv()).await
    {
        assert!(frames_in.open(&f).is_ok());
        relayed.push(f);
    }

    // 9. Done, from her phone: the agent continues, signed in, and never sees what she typed.
    let done = up.seal(&serde_json::to_vec(&InputEvent::Done).unwrap());
    sent.push(done.clone());
    assert!(p.takeover_input(&lease, &done).await.unwrap());
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("Signed in as"), "{read}");
    assert!(!read.contains(PASSWORD) && !read.contains(OTP), "{read}");
    assert!(read.contains("debug: password [hidden]"), "{read}");
    p.browser(
        &lease,
        BrowserCall::Open {
            url: format!("{base}/account"),
        },
    )
    .await
    .unwrap();
    let read = p.browser(&lease, BrowserCall::Read).await.unwrap();
    assert!(read.contains("Order 1042: 2 blue mugs, shipped."), "{read}");
    p.live_stop(&lease).await.unwrap();

    // 10. Nothing outside the VM held her password in the clear: not the relayed frames or
    //     input, not the egress log.
    let pw = PASSWORD.as_bytes();
    for b in relayed.iter().chain(sent.iter()) {
        assert!(!b.windows(pw.len()).any(|w| w == pw));
    }
    let log = serde_json::to_string(&egress.log()).unwrap();
    assert!(!log.contains(PASSWORD) && !log.contains(OTP) && !log.contains("/login"));
    let (signed, unsigned) = {
        let s = shop_log.lock().unwrap();
        (s.signed, s.unsigned)
    };
    assert!(signed >= 8 && unsigned == 0, "every request was signed");
    println!(
        "shop: {} signed requests, {} unsigned; {} frames relayed during the takeover",
        signed,
        unsigned,
        relayed.len()
    );
    p.release(lease).await.unwrap();
}

fn read_u64(p: &std::path::Path) -> u64 {
    read(p).parse().unwrap_or(0)
}
