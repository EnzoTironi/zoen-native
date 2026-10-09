//! The proxy: HTTP CONNECT tunnels and plain-HTTP forwarding, one lease per credential.

use crate::policy::{
    host_matches, is_forbidden, EgressRule, SecretBinding, BLOCKED_PORTS, PLACEHOLDER_PREFIX,
};
use crate::tls::{LeaseCa, Upstream};
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
    /// Test only: roots trusted for upstream TLS on top of Mozilla's (a journey's own origin).
    pub extra_upstream_roots_pem: Vec<String>,
}

impl Default for EgressConfig {
    fn default() -> Self {
        EgressConfig {
            max_requests_per_minute: 600,
            max_head_bytes: 16 * 1024,
            connect_timeout: Duration::from_secs(10),
            allow_loopback_upstreams: false,
            extra_upstream_roots_pem: vec![],
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
    cas: HashMap<String, Arc<LeaseCa>>,
    log: Vec<LogEntry>,
}

pub struct Egress {
    state: Mutex<State>,
    config: EgressConfig,
    resolver: Resolver,
    secrets: Arc<dyn SecretSource>,
    approvals: Arc<dyn ApprovalSink>,
    upstream: Upstream,
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
    /// Origin-form path for absolute-form requests; `None` for CONNECT.
    path: Option<String>,
    /// `https://` absolute form: the proxy opens the TLS connection to the origin.
    tls: bool,
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
        let upstream = Upstream::new(&config.extra_upstream_roots_pem)
            .expect("upstream TLS roots (extra roots must be valid PEM)");
        Arc::new(Egress {
            state: Mutex::new(State::default()),
            config,
            resolver,
            secrets,
            approvals,
            upstream,
        })
    }

    /// Starts allowing traffic for a lease. Rules are validated here, so a bad manifest fails
    /// before the sandbox runs.
    pub fn register(&self, policy: LeasePolicy) -> Result<(), String> {
        for r in &policy.rules {
            r.validate()?;
        }
        // A tool with secrets gets its own CA, limited to the hosts those secrets are for.
        let hosts: Vec<String> = policy
            .secrets
            .iter()
            .flat_map(|b| b.hosts.iter().cloned())
            .collect();
        let ca = if hosts.is_empty() {
            None
        } else {
            Some(Arc::new(LeaseCa::new(&policy.lease, &hosts)?))
        };
        let mut st = self.state.lock().unwrap();
        match ca {
            Some(ca) => st.cas.insert(policy.lease.clone(), ca),
            None => st.cas.remove(&policy.lease),
        };
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
        st.cas.remove(lease);
        st.pending.retain(|_, p| p.lease != lease);
    }

    /// The certificate of the lease's CA, for the sandbox's trust bundle; `None` when the
    /// tool has no secrets (then nothing is ever intercepted).
    pub fn lease_ca_pem(&self, lease: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .cas
            .get(lease)
            .map(|c| c.cert_pem().to_string())
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
        let (buf, head_end) = match read_head(&mut conn, self.config.max_head_bytes).await? {
            Head::Closed => return Ok(()),
            Head::TooBig => {
                return self
                    .refuse(&mut conn, &ctx, Decision::BadRequest, None)
                    .await
            }
            Head::Complete(buf, end) => (buf, end),
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
        let allowed = self.allowed(&policy, &target.host, target.port, &method);
        if !allowed {
            let id = self.open_card(&policy, &target, &method);
            return self
                .refuse(&mut conn, &ctx, Decision::NeedsApproval, Some(&id))
                .await;
        }

        // Secrets: placeholders become values only for hosts the secret is bound to.
        let mut out_headers = vec![];
        if target.path.is_some() {
            match self.rewrite_headers(&policy, &target.host, &hdrs) {
                Ok(h) => out_headers = h,
                Err(d) => return self.refuse(&mut conn, &ctx, d, None).await,
            }
        }

        // Resolve ourselves; every address must be public.
        let upstream = match self.dial(&target.host, target.port).await {
            Ok(s) => s,
            Err(d) => return self.refuse(&mut conn, &ctx, d, None).await,
        };

        if target.path.is_none() {
            conn.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
            let ca = {
                let st = self.state.lock().unwrap();
                st.cas.get(&policy.lease).cloned()
            };
            let intercept = ca.filter(|_| policy.secrets.iter().any(|b| b.allows(&target.host)));
            let Some(ca) = intercept else {
                // An opaque tunnel.
                let mut upstream = upstream;
                if !leftover.is_empty() {
                    upstream.write_all(&leftover).await?;
                }
                let (a, b) = tokio::io::copy_bidirectional(&mut conn, &mut upstream)
                    .await
                    .unwrap_or((0, 0));
                self.record(&ctx, Decision::Allowed, leftover.len() as u64 + a, b);
                return Ok(());
            };
            // Intercepted: TLS with the lease's CA towards the sandbox, real TLS upstream.
            let acceptor = match ca.acceptor(&target.host) {
                Ok(a) => a,
                Err(_) => {
                    self.record(&ctx, Decision::UpstreamError, 0, 0);
                    return Ok(());
                }
            };
            let prefixed = Prefixed {
                prefix: leftover,
                inner: conn,
            };
            let mut inner = match acceptor.accept(prefixed).await {
                Ok(s) => s,
                Err(_) => {
                    self.record(&ctx, Decision::BadRequest, 0, 0);
                    return Ok(());
                }
            };
            let (buf, head_end) = match read_head(&mut inner, self.config.max_head_bytes).await? {
                Head::Closed => return Ok(()),
                Head::TooBig => {
                    return self
                        .refuse(&mut inner, &ctx, Decision::BadRequest, None)
                        .await
                }
                Head::Complete(buf, end) => (buf, end),
            };
            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);
            if !matches!(
                req.parse(&buf[..head_end]),
                Ok(httparse::Status::Complete(_))
            ) {
                return self
                    .refuse(&mut inner, &ctx, Decision::BadRequest, None)
                    .await;
            }
            let method = req.method.unwrap_or("").to_string();
            let path = req.path.unwrap_or("/").to_string();
            let hdrs: Vec<(String, Vec<u8>)> = req
                .headers
                .iter()
                .map(|h| (h.name.to_string(), h.value.to_vec()))
                .collect();
            ctx.method = Some(method.clone());
            // Method rules apply to what's inside the tunnel too.
            if !self.allowed(&policy, &target.host, target.port, &method) {
                let id = self.open_card(&policy, &target, &method);
                return self
                    .refuse(&mut inner, &ctx, Decision::NeedsApproval, Some(&id))
                    .await;
            }
            let out_headers = match self.rewrite_headers(&policy, &target.host, &hdrs) {
                Ok(h) => h,
                Err(d) => return self.refuse(&mut inner, &ctx, d, None).await,
            };
            let mut tls_up = match self.upstream.connect(&target.host, upstream).await {
                Ok(s) => s,
                Err(_) => {
                    return self
                        .refuse(&mut inner, &ctx, Decision::UpstreamError, None)
                        .await
                }
            };
            let head = request_head(&method, &path, &out_headers);
            let (a, b) = forward(&mut inner, &mut tls_up, &head, &buf[head_end..]).await?;
            self.record(&ctx, Decision::Allowed, a, b);
            return Ok(());
        }

        let head = request_head(&method, target.path.as_deref().unwrap_or("/"), &out_headers);
        let (a, b) = if target.tls {
            let mut up = match self.upstream.connect(&target.host, upstream).await {
                Ok(s) => s,
                Err(_) => {
                    return self
                        .refuse(&mut conn, &ctx, Decision::UpstreamError, None)
                        .await
                }
            };
            forward(&mut conn, &mut up, &head, &leftover).await?
        } else {
            let mut up = upstream;
            forward(&mut conn, &mut up, &head, &leftover).await?
        };
        self.record(&ctx, Decision::Allowed, a, b);
        Ok(())
    }

    /// The manifest's rules, the owner's tool-wide approvals, or a one-time approval.
    fn allowed(&self, policy: &LeasePolicy, host: &str, port: u16, method: &str) -> bool {
        let mut st = self.state.lock().unwrap();
        let tool_wide = st
            .tool_wide
            .get(&(policy.owner.clone(), policy.tool.clone()))
            .map(|rs| rs.iter().any(|r| r.matches(host, port, method)))
            .unwrap_or(false);
        let Some(l) = st.leases.get_mut(&policy.lease) else {
            return false;
        };
        if l.policy.rules.iter().any(|r| r.matches(host, port, method)) || tool_wide {
            return true;
        }
        if let Some(i) = l.once.iter().position(|r| r.matches(host, port, method)) {
            l.once.remove(i);
            return true;
        }
        false
    }

    fn rewrite_headers(
        &self,
        policy: &LeasePolicy,
        host: &str,
        hdrs: &[(String, Vec<u8>)],
    ) -> Result<Vec<(String, Vec<u8>)>, Decision> {
        let mut out = Vec::with_capacity(hdrs.len());
        for (name, value) in hdrs {
            let lname = name.to_ascii_lowercase();
            if lname.starts_with("proxy-") || lname == "connection" || lname == "keep-alive" {
                continue;
            }
            out.push((name.clone(), self.inject(policy, host, value)?));
        }
        Ok(out)
    }

    /// Resolves `host` ourselves and connects, refusing private and metadata addresses.
    async fn dial(&self, host: &str, port: u16) -> Result<TcpStream, Decision> {
        let addrs = match self.resolver.resolve(host, port).await {
            Ok(a) if !a.is_empty() => a,
            _ => return Err(Decision::UpstreamError),
        };
        let forbidden = addrs.iter().any(|a| {
            is_forbidden(a.ip()) && !(self.config.allow_loopback_upstreams && a.ip().is_loopback())
        });
        if forbidden {
            return Err(Decision::PrivateAddress);
        }
        match tokio::time::timeout(self.config.connect_timeout, TcpStream::connect(addrs[0])).await
        {
            Ok(Ok(s)) => Ok(s),
            _ => Err(Decision::UpstreamError),
        }
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

enum Head {
    Closed,
    TooBig,
    Complete(Vec<u8>, usize),
}

async fn read_head<S: AsyncRead + Unpin>(conn: &mut S, max: usize) -> std::io::Result<Head> {
    let mut buf = Vec::with_capacity(4096);
    loop {
        let mut chunk = [0u8; 4096];
        let n = conn.read(&mut chunk).await?;
        if n == 0 {
            return Ok(Head::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(Head::Complete(buf, i + 4));
        }
        if buf.len() > max {
            return Ok(Head::TooBig);
        }
    }
}

/// One request upstream: `Connection: close`, so one request per connection.
fn request_head(method: &str, path: &str, headers: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut head = format!("{method} {path} HTTP/1.1\r\n").into_bytes();
    for (n, v) in headers {
        head.extend_from_slice(n.as_bytes());
        head.extend_from_slice(b": ");
        head.extend_from_slice(v);
        head.extend_from_slice(b"\r\n");
    }
    head.extend_from_slice(b"Connection: close\r\n\r\n");
    head
}

async fn forward<C, U>(
    conn: &mut C,
    upstream: &mut U,
    head: &[u8],
    leftover: &[u8],
) -> std::io::Result<(u64, u64)>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    upstream.write_all(head).await?;
    if !leftover.is_empty() {
        upstream.write_all(leftover).await?;
    }
    let (a, b) = tokio::io::copy_bidirectional(conn, upstream)
        .await
        .unwrap_or((0, 0));
    Ok(((head.len() + leftover.len()) as u64 + a, b))
}

/// A stream with bytes already read in front of it (the TLS ClientHello can arrive together
/// with the CONNECT head).
struct Prefixed<S> {
    prefix: Vec<u8>,
    inner: S,
}

impl<S: AsyncRead + Unpin> AsyncRead for Prefixed<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if !self.prefix.is_empty() {
            let n = self.prefix.len().min(buf.remaining());
            buf.put_slice(&self.prefix[..n]);
            self.prefix.drain(..n);
            return std::task::Poll::Ready(Ok(()));
        }
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Prefixed<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
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
            tls: false,
        });
    }
    let (rest, tls, default_port) = if let Some(r) = raw.strip_prefix("http://") {
        (r, false, 80)
    } else {
        (raw.strip_prefix("https://")?, true, 443)
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let authority = authority
        .rsplit_once('@')
        .map(|(_, a)| a)
        .unwrap_or(authority);
    let (host, port) = split_host_port(authority, default_port)?;
    (!host.is_empty()).then_some(Target {
        host,
        port,
        path: Some(path),
        tls,
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
        let t = parse_target("GET", "https://x.test/a").unwrap();
        assert_eq!((t.port, t.tls, t.path.as_deref()), (443, true, Some("/a")));
        assert!(parse_target("GET", "ftp://x.test/").is_none());
        assert!(parse_target("GET", "/relative").is_none());
        let t = parse_target("CONNECT", "[::1]:443").unwrap();
        assert_eq!(t.host, "[::1]");
    }
}
