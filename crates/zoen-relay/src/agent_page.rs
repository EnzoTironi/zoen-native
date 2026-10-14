//! Public agent page for people without the app: name, avatar, purpose, open / install.
//! Served at `GET /a/{target}` where `target` is a handle or an invite code.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use roda_types::IdentityKind;

use crate::{db, Shared};

/// Soft cream surface, concentric radii, no fluff — PT-BR, plain words.
pub async fn page(State(st): State<Shared>, Path(target): Path<String>) -> Response {
    match resolve(&st, &target).await {
        Some(card) => Html(render(&card)).into_response(),
        None => (StatusCode::NOT_FOUND, Html(not_found())).into_response(),
    }
}

struct Card {
    name: String,
    handle: String,
    bio: String,
    tint: String,
    /// Path segment for deep link (`handle` or invite `code`).
    target: String,
}

async fn resolve(st: &Shared, raw: &str) -> Option<Card> {
    let target = raw.trim().trim_start_matches('@');
    if target.is_empty() || target.len() > 64 {
        return None;
    }
    // Invite code first (uppercase alnum), then handle.
    if roda_proto::valid_invite_code(target) {
        if let Ok(info) = st.log.preview_invite(target).await {
            if let Some(agent_id) = info.agent {
                let profiles = db::profiles(&st.pool, &[agent_id]).await.ok()?;
                let p = profiles.into_iter().next()?;
                if p.kind != IdentityKind::Agent {
                    return None;
                }
                return Some(card_from(&p, target));
            }
        }
    }
    let handle = target.to_lowercase();
    let found = db::lookup(&st.pool, &handle, false).await.ok()?;
    let p = found
        .into_iter()
        .find(|i| i.kind == IdentityKind::Agent && i.handle == handle)?;
    Some(card_from(&p, &p.handle))
}

fn card_from(p: &roda_types::Identity, target: &str) -> Card {
    let _ = &p.glyph;
    Card {
        name: p.name.clone(),
        handle: p.handle.clone(),
        bio: if p.bio.trim().is_empty() {
            "Um agente no Zoen.".into()
        } else {
            p.bio.clone()
        },
        tint: if p.tint_hex.is_empty() {
            "#4F7CFF".into()
        } else {
            p.tint_hex.clone()
        },
        target: target.to_string(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render(c: &Card) -> String {
    let name = esc(&c.name);
    let handle = esc(&c.handle);
    let bio = esc(&c.bio);
    let tint = esc(&c.tint);
    let target = esc(&c.target);
    let deep = format!("zoen://agent/{target}");
    let https = format!("https://tryzoen.com/a/{target}");
    format!(
        r##"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="theme-color" content="#faf7f0">
<title>{name} · Zoen</title>
<style>
  :root {{
    color-scheme: light;
    --ink: #1d1b16;
    --muted: #7a7466;
    --paper: #faf7f0;
    --card: #ffffff;
    --line: oklch(0 0 0 / 0.1);
    --tint: {tint};
  }}
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; min-height: 100%; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    background: var(--paper);
    color: var(--ink);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }}
  .shell {{
    width: min(100%, 420px);
    background: var(--card);
    border-radius: 28px;
    padding: 16px;
    border: 1px solid transparent;
    box-shadow:
      0 1px 2px oklch(0 0 0 / 0.04),
      0 8px 24px oklch(0 0 0 / 0.06);
  }}
  .inner {{
    border-radius: 20px;
    padding: 28px 20px 24px;
    text-align: center;
  }}
  .avatar {{
    width: 88px; height: 88px; margin: 0 auto 16px;
    border-radius: 28px;
    background: linear-gradient(145deg, var(--tint), color-mix(in oklab, var(--tint) 55%, #1d1b16));
    outline: 1px solid oklch(0 0 0 / 0.1);
    outline-offset: -1px;
    display: grid; place-items: center;
    color: #fff; font-size: 36px; font-weight: 600;
  }}
  h1 {{
    font-size: 24px; font-weight: 700; letter-spacing: -0.02em;
    margin: 0 0 4px;
  }}
  .handle {{
    color: var(--muted); font-size: 14px; margin: 0 0 14px;
  }}
  .bio {{
    font-size: 16px; line-height: 1.45; color: var(--ink);
    margin: 0 0 28px;
  }}
  .actions {{
    display: flex; flex-direction: column; gap: 10px;
  }}
  a.btn {{
    display: block; text-decoration: none; text-align: center;
    padding: 14px 18px; border-radius: 16px;
    font-size: 16px; font-weight: 600;
    transition: transform 150ms ease-out, background-color 150ms ease-out, opacity 150ms ease-out;
    -webkit-tap-highlight-color: transparent;
    border: 1px solid transparent;
  }}
  a.btn:not(:disabled):active {{ transform: scale(0.96); }}
  a.primary {{
    background: var(--ink); color: #fff;
  }}
  a.secondary {{
    background: transparent; color: var(--ink);
    border-color: var(--line);
  }}
  @media (hover: hover) {{
    a.primary:hover {{ opacity: 0.92; }}
    a.secondary:hover {{ background: oklch(0 0 0 / 0.04); }}
  }}
  @media (prefers-reduced-motion: reduce) {{
    a.btn {{ transition: none; }}
    a.btn:not(:disabled):active {{ transform: none; }}
  }}
  .brand {{
    margin-top: 20px; font-size: 12px; color: var(--muted);
  }}
</style>
</head>
<body>
  <main class="shell">
    <div class="inner">
      <div class="avatar" aria-hidden="true">{name_initial}</div>
      <h1>{name}</h1>
      <p class="handle">@{handle}</p>
      <p class="bio">{bio}</p>
      <div class="actions">
        <a class="btn primary" href="{deep}" data-https="{https}">Abrir no app</a>
        <a class="btn secondary" href="/install">Instalar</a>
      </div>
      <p class="brand">Zoen</p>
    </div>
  </main>
  <script>
    (function () {{
      var a = document.querySelector("a.primary");
      if (!a) return;
      a.addEventListener("click", function (e) {{
        var deep = a.getAttribute("href");
        var https = a.getAttribute("data-https");
        var t = Date.now();
        setTimeout(function () {{
          if (Date.now() - t < 1600) location.href = "/install";
        }}, 1200);
        location.href = deep;
        e.preventDefault();
      }});
    }})();
  </script>
</body>
</html>
"##,
        name_initial = esc(&c
            .name
            .chars()
            .next()
            .map(|ch| ch.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into())),
        name = name,
        handle = handle,
        bio = bio,
        tint = tint,
        deep = esc(&deep),
        https = esc(&https),
    )
}

fn not_found() -> String {
    r##"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Agente não encontrado · Zoen</title>
<style>
  body { font-family: -apple-system, system-ui, sans-serif; background: #faf7f0; color: #1d1b16;
    display: grid; place-items: center; min-height: 100vh; margin: 0; padding: 24px; text-align: center; }
  h1 { font-size: 22px; margin: 0 0 8px; }
  p { color: #7a7466; margin: 0 0 20px; }
  a { color: #1d1b16; font-weight: 600; }
</style>
</head>
<body>
  <div>
    <h1>Esse agente não está aqui</h1>
    <p>O link pode ter expirado ou o endereço está errado.</p>
    <a href="/install">Instalar o Zoen</a>
  </div>
</body>
</html>
"##
    .into()
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

/// Where "Instalar" goes: Android → Google Play; everyone else gets a plain note.
pub async fn install(headers: axum::http::HeaderMap) -> Response {
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if ua.contains("android") {
        return axum::response::Redirect::temporary(
            "https://play.google.com/store/apps/details?id=xyz.tironi.zoen",
        )
        .into_response();
    }
    Html(r##"<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Instalar o Zoen</title>
<style>
  body { font-family: -apple-system, system-ui, sans-serif; background: #faf7f0; color: #1d1b16;
    display: grid; place-items: center; min-height: 100vh; margin: 0; padding: 24px; text-align: center; }
  h1 { font-size: 22px; margin: 0 0 8px; } p { color: #7a7466; margin: 0 0 16px; line-height: 1.45; }
  a { color: #1d1b16; font-weight: 600; }
</style></head><body><div>
  <h1>Instalar o Zoen</h1>
  <p>No Android, o Zoen está no Google Play.<br>No iPhone, ele chega em breve.</p>
  <a href="https://play.google.com/store/apps/details?id=xyz.tironi.zoen">Abrir no Google Play</a>
</div></body></html>
"##).into_response()
}
