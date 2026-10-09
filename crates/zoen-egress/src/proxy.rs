//! The proxy: HTTP CONNECT tunnels and plain-HTTP forwarding, one lease per credential.

use crate::policy::{
    host_matches, is_forbidden, EgressRule, SecretBinding, BLOCKED_PORTS, PLACEHOLDER_PREFIX,
};
use base64::Engine;
use roda_types::{ActionClass, AgentRequest, Capability, Grant, GrantScope, IdentityId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What one lease (one sandbox running one tool for one owner) may do.
#[derive(Clone, Debug)]
pub struct LeasePolicy {
    pub lease: String,
    /// Shared with the sandbox as `Proxy-Authorization: Basic base64(lease:token)`.
    pub token: String,
    pub owner: IdentityId,
    pub agent: IdentityId,
    pub tool: String,
    pub rules: Vec<EgressRule>,
    pub secrets: Vec<SecretBinding>,
}

/// Where real secret values come from. In production: the owner's vault, unsealed for this
/// lease only. Values never leave the proxy except inside requests to bound hosts.
pub trait SecretSource: Send + Sync {
    fn secret(&self, owner: &str, name: &str) -> Option<String>;
}

/// Receives approval cards for requests outside the allowlist (the runtime posts them as
/// `RequestOpened` to the owner).
pub trait ApprovalSink: Send + Sync {
    fn opened(&self, lease: &str, request: &AgentRequest);
}

/// How names become addresses. `Static` is for tests and for pinning in development.
#[derive(Clone, Debug, Default)]
pub enum Resolver {
    #[default]
    System,
    Static(HashMap<String, Vec<IpAddr>>),
}

impl Resolver {
    async fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        match self {
            Resolver::System => Ok(tokio::net::lookup_host((host, port)).await?.collect()),
            Resolver::Static(map) => Ok(map
                .get(&host.to_ascii_lowercase())
                .map(|ips| ips.iter().map(|ip| SocketAddr::new(*ip, port)).collect())
                .unwrap_or_default()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EgressConfig {
    pub max_requests_per_minute: u32,
    pub max_head_bytes: usize,
    pub connect_timeout: Duration,
    /// Test only: lets a loopback upstream through so journeys can run a local origin. Every
    /// other forbidden range stays forbidden.
    pub allow_loopback_upstreams: bool,
}

impl Default for EgressConfig {
    fn default() -> Self {
        EgressConfig {
            max_requests_per_minute: 600,
            max_head_bytes: 16 * 1024,
            connect_timeout: Duration::from_secs(10),
            allow_loopback_upstreams: false,
        }
    }
}

/// How far an owner's "yes" goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalScope {
    /// The next matching request only.
    Once,
    /// The rest of this lease (this task).
    Task,
    /// This tool, for this owner, from now on. Returns a Grant to record.
    AlwaysForTool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allowed,
    NeedsApproval,
    PrivateAddress,
    IpLiteral,
    BlockedPort,
    SecretNotBound,
    UnknownSecret,
    BadAuth,
    RateLimited,
    UpstreamError,
    BadRequest,
}

impl Decision {
    fn code(self) -> &'static str {
        match self {
            Decision::Allowed => "OK",
            Decision::NeedsApproval => "EGRESS_NEEDS_APPROVAL",
            Decision::PrivateAddress => "EGRESS_PRIVATE_ADDRESS",
            Decision::IpLiteral => "EGRESS_IP_LITERAL",
            Decision::BlockedPort => "EGRESS_BLOCKED_PORT",
            Decision::SecretNotBound => "EGRESS_SECRET_NOT_BOUND",
            Decision::UnknownSecret => "EGRESS_UNKNOWN_SECRET",
            Decision::BadAuth => "EGRESS_BAD_AUTH",
            Decision::RateLimited => "EGRESS_RATE_LIMITED",
            Decision::UpstreamError => "EGRESS_UPSTREAM_ERROR",
            Decision::BadRequest => "EGRESS_BAD_REQUEST",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Decision::Allowed => "200 OK",
            Decision::BadAuth => "407 Proxy Authentication Required",
            Decision::RateLimited => "429 Too Many Requests",
            Decision::UpstreamError => "502 Bad Gateway",
            Decision::BadRequest => "400 Bad Request",
            _ => "403 Forbidden",
        }
    }
}

/// One line of the egress log. Metadata only, by construction: there is no field for a path,
/// a query, a header or a body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub at_ms: i64,
    pub lease: Option<String>,
    pub tool: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub method: Option<String>,
    pub decision: Decision,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub ms: u64,
}

struct Pending {
    lease: String,
    owner: IdentityId,
    agent: IdentityId,
    tool: String,
    host: String,
    port: u16,
    method: String,
}

struct LeaseState {
    policy: LeasePolicy,
    once: Vec<EgressRule>,
    window_start: Instant,
    count: u32,
}

#[derive(Default)]
struct State {
    leases: HashMap<String, LeaseState>,
    pending: HashMap<String, Pending>,
    tool_wide: HashMap<(IdentityId, String), Vec<EgressRule>>,
    log: Vec<LogEntry>,
}

pub struct Egress {
    state: Mutex<State>,
    config: EgressConfig,
    resolver: Resolver,
    secrets: Arc<dyn SecretSource>,
    approvals: Arc<dyn ApprovalSink>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let (ha, hb) = (Sha256::digest(a), Sha256::digest(b));
    ha.iter()
        .zip(hb.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

struct Target {
    host: String,
    port: u16,
    /// Origin-form path for plain HTTP; `None` for CONNECT.
    path: Option<String>,
}

struct Ctx {
    lease: Option<String>,
    tool: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    method: Option<String>,
    started: Instant,
}

impl Egress {
    pub fn new(
        config: EgressConfig,
        resolver: Resolver,
        secrets: Arc<dyn SecretSource>,
        approvals: Arc<dyn ApprovalSink>,
    ) -> Arc<Self> {
        Arc::new(Egress {
            state: Mutex::new(State::default()),
            config,
            resolver,
            secrets,
            approvals,
        })
    }

    /// Starts allowing traffic for a lease. Rules are validated here, so a bad manifest fails
    /// before the sandbox runs.
    pub fn register(&self, policy: LeasePolicy) -> Result<(), String> {
        for r in &policy.rules {
            r.validate()?;
        }
        let mut st = self.state.lock().unwrap();
        st.leases.insert(
            policy.lease.clone(),
            LeaseState {
                policy,
                once: vec![],
                window_start: Instant::now(),
                count: 0,
            },
        );
        Ok(())
    }

    /// Stops a lease: its credential no longer works and its pending cards are dropped.
    pub fn revoke(&self, lease: &str) {
        let mut st = self.state.lock().unwrap();
        st.leases.remove(lease);
        st.pending.retain(|_, p| p.lease != lease);
    }

    /// The owner said yes. With `AlwaysForTool`, returns the Grant to record (`net:<host>`
    /// for that agent), which also applies to this tool's future leases.
    pub fn approve(&self, request: &str, scope: ApprovalScope) -> Option<Grant> {
        let mut st = self.state.lock().unwrap();
        let p = st.pending.remove(request)?;
        let rule = EgressRule {
            host: p.host.clone(),
            ports: vec![p.port],
            methods: vec![],
        };
        let mut grant = None;
        match scope {
            ApprovalScope::Once => {
                if let Some(l) = st.leases.get_mut(&p.lease) {
                    l.once.push(rule);
                }
            }
            ApprovalScope::Task => {
                if let Some(l) = st.leases.get_mut(&p.lease) {
                    l.policy.rules.push(rule);
                }
            }
            ApprovalScope::AlwaysForTool => {
                st.tool_wide
                    .entry((p.owner.clone(), p.tool.clone()))
                    .or_default()
                    .push(rule);
                grant = Some(Grant {
                    id: roda_types::new_id("grant"),
                    grantor: p.owner.clone(),
                    grantee: Some(p.agent.clone()),
                    scope: GrantScope::Everywhere,
                    capability: Capability::Device {
                        capability: format!("net:{}", p.host),
                        purpose: format!("tool {}", p.tool),
                    },
                    expires_at_ms: None,
                });
            }
        }
        grant
    }

    /// The owner said no: the card closes and the host stays refused.
    pub fn deny(&self, request: &str) {
        self.state.lock().unwrap().pending.remove(request);
    }

    pub fn log(&self) -> Vec<LogEntry> {
        self.state.lock().unwrap().log.clone()
    }

    pub async fn serve(self: Arc<Self>, listener: TcpListener) {
        loop {
            let Ok((conn, _)) = listener.accept().await else {
                continue;
            };
            let me = self.clone();
            tokio::spawn(async move {
                let _ = me.handle(conn, None).await;
            });
        }
    }

    /// Serves one connection that already belongs to `lease`: the sandbox node agent knows
    /// which VM a vsock connection came from, so the guest never holds a proxy credential.
    /// Everything else (allowlist, approvals, secrets, rate limit, log) is the same.
    pub async fn serve_stream<S>(self: Arc<Self>, conn: S, lease: &str) -> std::io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        self.handle(conn, Some(lease)).await
    }

    fn record(&self, ctx: &Ctx, decision: Decision, up: u64, down: u64) {
        self.state.lock().unwrap().log.push(LogEntry {
            at_ms: now_ms(),
            lease: ctx.lease.clone(),
            tool: ctx.tool.clone(),
            host: ctx.host.clone(),
            port: ctx.port,
            method: ctx.method.clone(),
            decision,
            bytes_up: up,
            bytes_down: down,
            ms: ctx.started.elapsed().as_millis() as u64,
        });
    }

    async fn refuse<S: AsyncWrite + Unpin>(
        &self,
        conn: &mut S,
        ctx: &Ctx,
        d: Decision,
        request: Option<&str>,
    ) -> std::io::Result<()> {
        self.record(ctx, d, 0, 0);
        let body = serde_json::json!({ "code": d.code(), "request": request }).to_string();
        let resp = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            d.status(),
            body.len(),
            body
        );
        conn.write_all(resp.as_bytes()).await?;
        conn.shutdown().await
    }

    async fn handle<S>(self: Arc<Self>, mut conn: S, preauth: Option<&str>) -> std::io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        let mut ctx = Ctx {
            lease: None,
            tool: None,
            host: None,
            port: None,
            method: None,
            started: Instant::now(),
        };

        // Read the request head.
        let mut buf = Vec::with_capacity(4096);
        let head_end = loop {
            let mut chunk = [0u8; 4096];
            let n = conn.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            if buf.len() > self.config.max_head_bytes {
                return self
                    .refuse(&mut conn, &ctx, Decision::BadRequest, None)
                    .await;
            }
        };
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        if !matches!(
            req.parse(&buf[..head_end]),
            Ok(httparse::Status::Complete(_))
        ) {
            return self
                .refuse(&mut conn, &ctx, Decision::BadRequest, None)
                .await;
        }
        let method = req.method.unwrap_or("").to_string();
        let target_raw = req.path.unwrap_or("").to_string();
        let hdrs: Vec<(String, Vec<u8>)> = req
            .headers
            .iter()
            .map(|h| (h.name.to_string(), h.value.to_vec()))
            .collect();
        let leftover = buf[head_end..].to_vec();
        ctx.method = Some(method.clone());

        // Who is asking.
        let auth = hdrs
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("proxy-authorization"))
            .and_then(|(_, v)| std::str::from_utf8(v).ok())
            .and_then(|v| v.strip_prefix("Basic "))
            .and_then(|b| {
                base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .ok()
            })
            .and_then(|b| String::from_utf8(b).ok());
        let (lease_id, token) = match (preauth, auth.as_deref().and_then(|a| a.split_once(':'))) {
            (Some(lease), _) => (lease, None),
            (None, Some((lease, token))) => (lease, Some(token)),
            (None, None) => return self.refuse(&mut conn, &ctx, Decision::BadAuth, None).await,
        };
        let policy = {
            let mut st = self.state.lock().unwrap();
            let max = self.config.max_requests_per_minute;
            match st.leases.get_mut(lease_id) {
                Some(l) if token.is_none_or(|t| ct_eq(l.policy.token.as_bytes(), t.as_bytes())) => {
                    if l.window_start.elapsed() >= Duration::from_secs(60) {
                        l.window_start = Instant::now();
                        l.count = 0;
                    }
                    l.count += 1;
                    if l.count > max {
                        Err(Decision::RateLimited)
                    } else {
                        Ok(l.policy.clone())
                    }
                }
                _ => Err(Decision::BadAuth),
            }
        };
        let policy = match policy {
            Ok(p) => p,
            Err(d) => return self.refuse(&mut conn, &ctx, d, None).await,
        };
        ctx.lease = Some(policy.lease.clone());
        ctx.tool = Some(policy.tool.clone());

        // Where to.
        let Some(target) = parse_target(&method, &target_raw) else {
            return self
                .refuse(&mut conn, &ctx, Decision::BadRequest, None)
                .await;
        };
        ctx.host = Some(target.host.clone());
        ctx.port = Some(target.port);
        if target
            .host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok()
        {
            return self
                .refuse(&mut conn, &ctx, Decision::IpLiteral, None)
                .await;
        }
        if BLOCKED_PORTS.contains(&target.port) {
            return self
                .refuse(&mut conn, &ctx, Decision::BlockedPort, None)
                .await;
        }

        // Allowlist: the manifest, plus what the owner approved.
        let allowed = {
            let mut st = self.state.lock().unwrap();
            let tool_wide = st
                .tool_wide
                .get(&(policy.owner.clone(), policy.tool.clone()))
                .map(|rs| {
                    rs.iter()
                        .any(|r| r.matches(&target.host, target.port, &method))
                })
                .unwrap_or(false);
            let l = st
                .leases
                .get_mut(&policy.lease)
                .expect("lease checked above");
            if l.policy
                .rules
                .iter()
                .any(|r| r.matches(&target.host, target.port, &method))
                || tool_wide
            {
                true
            } else if let Some(i) = l
                .once
                .iter()
                .position(|r| r.matches(&target.host, target.port, &method))
            {
                l.once.remove(i);
                true
            } else {
                false
            }
        };
        if !allowed {
            let id = self.open_card(&policy, &target, &method);
            return self
                .refuse(&mut conn, &ctx, Decision::NeedsApproval, Some(&id))
                .await;
        }

        // Secrets: placeholders become values only for hosts the secret is bound to.
        let mut out_headers = Vec::with_capacity(hdrs.len());
        if target.path.is_some() {
            for (name, value) in &hdrs {
                let lname = name.to_ascii_lowercase();
                if lname.starts_with("proxy-") || lname == "connection" || lname == "keep-alive" {
                    continue;
                }
                match self.inject(&policy, &target.host, value) {
                    Ok(v) => out_headers.push((name.clone(), v)),
                    Err(d) => return self.refuse(&mut conn, &ctx, d, None).await,
                }
            }
        }

        // Resolve ourselves; every address must be public.
        let addrs = match self.resolver.resolve(&target.host, target.port).await {
            Ok(a) if !a.is_empty() => a,
            _ => {
                return self
                    .refuse(&mut conn, &ctx, Decision::UpstreamError, None)
                    .await
            }
        };
        let forbidden = addrs.iter().any(|a| {
            is_forbidden(a.ip()) && !(self.config.allow_loopback_upstreams && a.ip().is_loopback())
        });
        if forbidden {
            return self
                .refuse(&mut conn, &ctx, Decision::PrivateAddress, None)
                .await;
        }
        let upstream =
            match tokio::time::timeout(self.config.connect_timeout, TcpStream::connect(addrs[0]))
                .await
            {
                Ok(Ok(s)) => s,
                _ => {
                    return self
                        .refuse(&mut conn, &ctx, Decision::UpstreamError, None)
                        .await
                }
            };
        let mut upstream = upstream;

        let mut up = leftover.len() as u64;
        if target.path.is_none() {
            conn.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
        } else {
            let mut head = format!(
                "{} {} HTTP/1.1\r\n",
                method,
                target.path.as_deref().unwrap_or("/")
            )
            .into_bytes();
            for (n, v) in &out_headers {
                head.extend_from_slice(n.as_bytes());
                head.extend_from_slice(b": ");
                head.extend_from_slice(v);
                head.extend_from_slice(b"\r\n");
            }
            head.extend_from_slice(b"Connection: close\r\n\r\n");
            up += head.len() as u64;
            upstream.write_all(&head).await?;
        }
        if !leftover.is_empty() {
            upstream.write_all(&leftover).await?;
        }
        let (a, b) = tokio::io::copy_bidirectional(&mut conn, &mut upstream)
            .await
            .unwrap_or((0, 0));
        self.record(&ctx, Decision::Allowed, up + a, b);
        Ok(())
    }

    fn inject(&self, policy: &LeasePolicy, host: &str, value: &[u8]) -> Result<Vec<u8>, Decision> {
        let Ok(text) = std::str::from_utf8(value) else {
            return Ok(value.to_vec());
        };
        if !text.contains(PLACEHOLDER_PREFIX) {
            return Ok(value.to_vec());
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find(PLACEHOLDER_PREFIX) {
            out.push_str(&rest[..i]);
            let after = &rest[i + PLACEHOLDER_PREFIX.len()..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(after.len());
            let name = &after[..end];
            let binding = policy
                .secrets
                .iter()
                .find(|b| b.name == name)
                .ok_or(Decision::UnknownSecret)?;
            if !binding.allows(host) {
                return Err(Decision::SecretNotBound);
            }
            let secret = self
                .secrets
                .secret(&policy.owner, name)
                .ok_or(Decision::UnknownSecret)?;
            out.push_str(&secret);
            rest = &after[end..];
        }
        out.push_str(rest);
        Ok(out.into_bytes())
    }

    fn open_card(&self, policy: &LeasePolicy, target: &Target, method: &str) -> String {
        let mut st = self.state.lock().unwrap();
        if let Some((id, _)) = st.pending.iter().find(|(_, p)| {
            p.lease == policy.lease
                && host_matches(&p.host, &target.host)
                && p.port == target.port
                && p.method.eq_ignore_ascii_case(method)
        }) {
            return id.clone();
        }
        let id = roda_types::new_id("req");
        let hash = hex::encode(Sha256::digest(
            format!(
                "egress/1|{}|{}|{}|{}|{}",
                policy.lease, policy.tool, target.host, target.port, method
            )
            .as_bytes(),
        ));
        let request = AgentRequest {
            id: id.clone(),
            agent: policy.agent.clone(),
            title: format!("Permitir acesso a {}", target.host),
            detail: format!(
                "A ferramenta {} quer se conectar a {} (porta {}), que não está na lista dela.",
                policy.tool, target.host, target.port
            ),
            audience: target.host.clone(),
            action: ActionClass::External,
            content_hash: hash,
            item: None,
            line: None,
        };
        st.pending.insert(
            id.clone(),
            Pending {
                lease: policy.lease.clone(),
                owner: policy.owner.clone(),
                agent: policy.agent.clone(),
                tool: policy.tool.clone(),
                host: target.host.clone(),
                port: target.port,
                method: method.to_string(),
            },
        );
        drop(st);
        self.approvals.opened(&policy.lease, &request);
        id
    }
}

fn split_host_port(authority: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None => default_port,
        };
        return Some((format!("[{h}]"), port));
    }
    match authority.rsplit_once(':') {
        Some((h, p)) => Some((h.to_ascii_lowercase(), p.parse().ok()?)),
        None => Some((authority.to_ascii_lowercase(), default_port)),
    }
}

fn parse_target(method: &str, raw: &str) -> Option<Target> {
    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = split_host_port(raw, 443)?;
        return (!host.is_empty()).then_some(Target {
            host,
            port,
            path: None,
        });
    }
    // P0 forwards plain HTTP only; HTTPS goes through CONNECT.
    let rest = raw.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let authority = authority
        .rsplit_once('@')
        .map(|(_, a)| a)
        .unwrap_or(authority);
    let (host, port) = split_host_port(authority, 80)?;
    (!host.is_empty()).then_some(Target {
        host,
        port,
        path: Some(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_parse() {
        let t = parse_target("CONNECT", "api.github.com:443").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.path),
            ("api.github.com", 443, None)
        );
        let t = parse_target("GET", "http://Example.test:8080/a?b").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.path.as_deref()),
            ("example.test", 8080, Some("/a?b"))
        );
        assert!(parse_target("GET", "https://x.test/").is_none());
        assert!(parse_target("GET", "/relative").is_none());
        let t = parse_target("CONNECT", "[::1]:443").unwrap();
        assert_eq!(t.host, "[::1]");
    }
}
