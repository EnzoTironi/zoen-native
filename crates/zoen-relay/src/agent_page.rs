//! Public agent page for people without the app: name, avatar, purpose, open / install.
//! Served at `GET /a/{target}` where `target` is a handle or an invite code.
//!
//! Every page here is static HTML with a strict CSP: inline style and script are allowed
//! only by hash, nothing is cached, nothing leaks a referrer, and crawlers skip it.

use std::sync::LazyLock;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use roda_types::{Identity, IdentityKind};
use sha2::{Digest, Sha256};

use crate::{db, ClientIp, Shared};

const DEFAULT_TINT: &str = "#4F7CFF";

pub async fn page(
    State(st): State<Shared>,
    ClientIp(ip): ClientIp,
    Path(target): Path<String>,
) -> Response {
    if let Err(wait) = st.limits.agent_page_ip.check(&ip) {
        let mut res = doc(
            StatusCode::TOO_MANY_REQUESTS,
            "Calma · Zoen",
            PLAIN_CSS,
            "<div><h1>Muitas visitas seguidas</h1><p>Espera um pouquinho e tenta de novo.</p></div>",
            None,
        );
        let secs = wait.as_secs().max(1).to_string();
        if let Ok(v) = HeaderValue::from_str(&secs) {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return res;
    }
    match resolve(&st, &target).await {
        Some(card) => card.render(),
        None => doc(
            StatusCode::NOT_FOUND,
            "Agente não encontrado · Zoen",
            PLAIN_CSS,
            r#"<div><h1>Esse agente não está aqui</h1><p>O link pode ter expirado ou o endereço está errado.</p><a href="/install">Instalar o Zoen</a></div>"#,
            None,
        ),
    }
}

struct Card {
    name: String,
    handle: String,
    bio: String,
    tint: String,
    /// The agent's glyph (emoji) or the initial of its name.
    mark: String,
    /// Path segment for the links (`handle` or invite `code`).
    target: String,
}

async fn resolve(st: &Shared, raw: &str) -> Option<Card> {
    let target = raw.trim().trim_start_matches('@');
    if target.is_empty() || target.len() > 64 {
        return None;
    }
    // Invite code first, then handle (the same order as the apps' `open_agent_link`).
    if roda_proto::valid_invite_code(target) {
        if let Ok(info) = st.log.preview_invite(target).await {
            let agent_id = info.agent?;
            let p = db::profiles(&st.pool, &[agent_id])
                .await
                .ok()?
                .into_iter()
                .next()?;
            return (p.kind == IdentityKind::Agent).then(|| Card::of(&p, &target.to_uppercase()));
        }
    }
    let handle = target.to_lowercase();
    db::lookup(&st.pool, &handle, false)
        .await
        .ok()?
        .into_iter()
        .find(|i| i.kind == IdentityKind::Agent && i.handle == handle)
        .map(|p| Card::of(&p, &p.handle))
}

/// `#RRGGBB` only; anything else could break out of the stylesheet.
fn valid_tint(t: &str) -> bool {
    t.len() == 7 && t.starts_with('#') && t[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The avatar the apps draw: an emoji glyph as is, a known symbol name as its emoji, else
/// the first letter of the name.
fn mark(p: &Identity) -> String {
    let glyph = p.glyph.as_deref().unwrap_or("").trim();
    if !glyph.is_empty() && !glyph.is_ascii() && glyph.chars().count() <= 4 {
        return glyph.to_string();
    }
    let symbol = match glyph {
        "scissors" => "✂️",
        "calendar" | "calendar.badge.clock" => "📅",
        "sparkles" => "✨",
        "fork.knife" => "🍴",
        "cart" | "bag" => "🛍️",
        "book" | "book.closed" => "📚",
        "music.note" => "🎵",
        "heart" => "❤️",
        _ => "",
    };
    if !symbol.is_empty() {
        return symbol.to_string();
    }
    p.name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

impl Card {
    fn of(p: &Identity, target: &str) -> Card {
        Card {
            name: p.name.clone(),
            handle: p.handle.clone(),
            bio: if p.bio.trim().is_empty() {
                "Um agente no Zoen.".into()
            } else {
                p.bio.clone()
            },
            tint: if valid_tint(&p.tint_hex) {
                p.tint_hex.clone()
            } else {
                DEFAULT_TINT.into()
            },
            mark: mark(p),
            target: target.to_string(),
        }
    }

    fn render(&self) -> Response {
        let target = esc(&self.target);
        let body = format!(
            r#"<main class="shell">
    <div class="inner">
      <div class="avatar" aria-hidden="true">{mark}</div>
      <h1>{name}</h1>
      <p class="handle">@{handle}</p>
      <p class="bio">{bio}</p>
      <div class="actions">
        <a id="open" class="btn primary" href="https://tryzoen.com/a/{target}" data-deep="zoen://agent/{target}">Abrir no app</a>
        <a class="btn secondary" href="/install">Instalar</a>
      </div>
      <p class="brand">Zoen</p>
    </div>
  </main>"#,
            mark = esc(&self.mark),
            name = esc(&self.name),
            handle = esc(&self.handle),
            bio = esc(&self.bio),
        );
        doc(
            StatusCode::OK,
            &format!("{} · Zoen", self.name),
            &CARD_CSS.replace("__TINT__", &self.tint),
            &body,
            Some(OPEN_SCRIPT),
        )
    }
}

/// "Abrir no app": iOS keeps the https universal link (the `href`); elsewhere it tries the
/// app's scheme and goes to `/install` only if the page is still showing 1.5 s later.
const OPEN_SCRIPT: &str = r#"(function () {
  var a = document.getElementById("open");
  if (!a) return;
  var ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return;
  a.addEventListener("click", function (e) {
    e.preventDefault();
    var timer = setTimeout(function () { location.href = "/install"; }, 1500);
    var cancel = function () { clearTimeout(timer); };
    document.addEventListener("visibilitychange", function () { if (document.hidden) cancel(); });
    window.addEventListener("pagehide", cancel);
    location.href = a.getAttribute("data-deep");
  });
})();"#;

static OPEN_SCRIPT_HASH: LazyLock<String> = LazyLock::new(|| csp_hash(OPEN_SCRIPT));

const CARD_CSS: &str = r#"  :root {
    color-scheme: light;
    --ink: #1d1b16;
    --muted: #7a7466;
    --paper: #faf7f0;
    --card: #ffffff;
    --line: oklch(0 0 0 / 0.1);
    --tint: __TINT__;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; min-height: 100%; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    background: var(--paper);
    color: var(--ink);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .shell {
    width: min(100%, 420px);
    background: var(--card);
    border-radius: 28px;
    padding: 16px;
    border: 1px solid transparent;
    box-shadow:
      0 1px 2px oklch(0 0 0 / 0.04),
      0 8px 24px oklch(0 0 0 / 0.06);
  }
  .inner {
    border-radius: 20px;
    padding: 28px 20px 24px;
    text-align: center;
  }
  .avatar {
    font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", inherit;
    width: 88px; height: 88px; margin: 0 auto 16px;
    border-radius: 28px;
    background: linear-gradient(145deg, var(--tint), color-mix(in oklab, var(--tint) 55%, #1d1b16));
    outline: 1px solid oklch(0 0 0 / 0.1);
    outline-offset: -1px;
    display: grid; place-items: center;
    color: #fff; font-size: 36px; font-weight: 600;
  }
  h1 {
    font-size: 24px; font-weight: 700; letter-spacing: -0.02em;
    margin: 0 0 4px;
  }
  .handle {
    color: var(--muted); font-size: 14px; margin: 0 0 14px;
  }
  .bio {
    font-size: 16px; line-height: 1.45; color: var(--ink);
    margin: 0 0 28px;
  }
  .actions {
    display: flex; flex-direction: column; gap: 10px;
  }
  a.btn {
    display: block; text-decoration: none; text-align: center;
    padding: 14px 18px; border-radius: 16px;
    font-size: 16px; font-weight: 600;
    transition: transform 150ms ease-out, background-color 150ms ease-out, opacity 150ms ease-out;
    -webkit-tap-highlight-color: transparent;
    border: 1px solid transparent;
  }
  a.btn:not(:disabled):active { transform: scale(0.96); }
  a.primary {
    background: var(--ink); color: #fff;
  }
  a.secondary {
    background: transparent; color: var(--ink);
    border-color: var(--line);
  }
  @media (hover: hover) {
    a.primary:hover { opacity: 0.92; }
    a.secondary:hover { background: oklch(0 0 0 / 0.04); }
  }
  @media (prefers-reduced-motion: reduce) {
    a.btn { transition: none; }
    a.btn:not(:disabled):active { transform: none; }
  }
  .brand {
    margin-top: 20px; font-size: 12px; color: var(--muted);
  }
"#;

const PLAIN_CSS: &str = r#"
  body { font-family: -apple-system, system-ui, sans-serif; background: #faf7f0; color: #1d1b16;
    display: grid; place-items: center; min-height: 100vh; margin: 0; padding: 24px; text-align: center; }
  h1 { font-size: 22px; margin: 0 0 8px; }
  p { color: #7a7466; margin: 0 0 16px; line-height: 1.45; }
  a { color: #1d1b16; font-weight: 600; }
"#;

fn csp_hash(s: &str) -> String {
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(s.as_bytes()))
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// One page: the inline `<style>` (and `<script>`) are allowed by hash and nothing else.
fn doc(status: StatusCode, title: &str, css: &str, body: &str, script: Option<&str>) -> Response {
    let script_src = match script {
        Some(_) => OPEN_SCRIPT_HASH.as_str(),
        None => "'none'",
    };
    let csp = format!(
        "default-src 'none'; style-src {}; script-src {script_src}; img-src 'self' data:; \
         base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        csp_hash(css)
    );
    let script = script
        .map(|s| format!("<script>{s}</script>"))
        .unwrap_or_default();
    let html = format!(
        r##"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="theme-color" content="#faf7f0">
<title>{title}</title>
<style>{css}</style>
</head>
<body>
  {body}
  {script}
</body>
</html>
"##,
        title = esc(title),
    );
    let mut res = (status, html).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    res
}

pub async fn assetlinks(State(st): State<Shared>) -> Response {
    if st.android_packages.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let body: Vec<_> = st
        .android_packages
        .iter()
        .map(|pkg| {
            serde_json::json!({
                "relation": ["delegate_permission/common.handle_all_urls"],
                "target": {
                    "namespace": "android_app",
                    "package_name": pkg.package_name,
                    "sha256_cert_fingerprints": pkg.fingerprints,
                }
            })
        })
        .collect();
    (
        [("content-type", "application/json")],
        serde_json::to_string(&body).unwrap_or_else(|_| "[]".into()),
    )
        .into_response()
}

/// Where "Instalar" goes: Android goes to the store listing once one is configured;
/// until then everyone gets the same "em breve" page.
pub async fn install(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let android = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.to_lowercase().contains("android"));
    if let (true, Some(url)) = (android, st.android_store_url.as_deref()) {
        return axum::response::Redirect::temporary(url).into_response();
    }
    let store = st
        .android_store_url
        .as_deref()
        .map(|u| format!(r#"<a href="{}">Abrir no Google Play</a>"#, esc(u)))
        .unwrap_or_default();
    let note = if store.is_empty() {
        "Em breve no iPhone e no Android."
    } else {
        "No Android, o Zoen está no Google Play.<br>No iPhone, ele chega em breve."
    };
    doc(
        StatusCode::OK,
        "Instalar o Zoen",
        PLAIN_CSS,
        &format!("<div><h1>Instalar o Zoen</h1><p>{note}</p>{store}</div>"),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_is_strict_hex() {
        assert!(valid_tint("#C45C26"));
        assert!(valid_tint("#abcdef"));
        for bad in ["", "red", "#FFF", "#GGGGGG", "#123456;}body{x:y", "C45C26X"] {
            assert!(!valid_tint(bad), "{bad}");
        }
    }

    #[test]
    fn script_hash_matches_what_the_page_carries() {
        assert!(OPEN_SCRIPT_HASH.starts_with("'sha256-"));
        assert!(!OPEN_SCRIPT.contains("</script"));
    }
}
