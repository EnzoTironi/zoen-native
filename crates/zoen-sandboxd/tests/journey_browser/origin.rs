//! Local shop origin for the browser authority journey.
use super::{
    verify_request, Arc, AsyncRead, AsyncWrite, CertificateDer, Mutex, PrivateKeyDer,
    PrivatePkcs8KeyDer, ShopLog, SignedAgent, SystemTime, TcpListener, OTP, PASSWORD, UNIX_EPOCH,
    USER,
};

pub(super) fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub(super) fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=utf-8><title>{title}</title></head>\
         <body style=\"font-family:sans-serif\">{body}</body></html>"
    )
}

pub(super) fn form_value(body: &str, key: &str) -> String {
    body.split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.replace('+', " ").replace("%40", "@"))
        .unwrap_or_default()
}

pub(super) async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
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
pub(super) async fn shop(agent: Arc<SignedAgent>, log: Arc<Mutex<ShopLog>>) -> (u16, String) {
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
