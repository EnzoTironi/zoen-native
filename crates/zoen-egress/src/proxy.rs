//! The proxy: HTTP CONNECT tunnels and plain-HTTP forwarding, one lease per credential.

use crate::policy::{
    host_matches, is_forbidden, EgressRule, SecretBinding, BLOCKED_PORTS, PLACEHOLDER_PREFIX,
};
use crate::tls::{LeaseCa, SandboxRoot, Upstream};
use crate::webbotauth::SignedAgent;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use types::{AgentRequest, IdentityId};

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
    /// Sign this lease's requests with Web Bot Auth (when the node has a [`SignedAgent`]).
    /// HTTPS to the allowlisted hosts is then intercepted so the signature can be added.
    pub sign_requests: bool,
    /// A browser lease: its CA is issued under the node's [`SandboxRoot`], which the
    /// browser template already trusts.
    pub browser: bool,
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
    /// The operator's Web Bot Auth identity; `None` signs nothing.
    pub signed_agent: Option<Arc<SignedAgent>>,
}

impl Default for EgressConfig {
    fn default() -> Self {
        EgressConfig {
            max_requests_per_minute: 600,
            max_head_bytes: 16 * 1024,
            connect_timeout: Duration::from_secs(10),
            allow_loopback_upstreams: false,
            extra_upstream_roots_pem: vec![],
            signed_agent: None,
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
    root: SandboxRoot,
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
            root: SandboxRoot::new().expect("sandbox root CA"),
        })
    }

    /// Starts allowing traffic for a lease. Rules are validated here, so a bad manifest fails
    /// before the sandbox runs.
    pub fn register(&self, policy: LeasePolicy) -> Result<(), String> {
        for r in &policy.rules {
            r.validate()?;
        }
        // A tool with secrets gets its own CA, limited to the hosts those secrets are for; a
        // signing lease's CA also covers its allowlisted hosts.
        let mut hosts: Vec<String> = policy
            .secrets
            .iter()
            .flat_map(|b| b.hosts.iter().cloned())
            .collect();
        if self.signs(&policy) {
            hosts.extend(policy.rules.iter().map(|r| r.host.clone()));
        }
        hosts.sort();
        hosts.dedup();
        let ca = if hosts.is_empty() {
            None
        } else {
            let root = policy.browser.then_some(&self.root);
            Some(Arc::new(LeaseCa::new(&policy.lease, &hosts, root)?))
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

    pub(super) fn signs(&self, policy: &LeasePolicy) -> bool {
        policy.sign_requests && self.config.signed_agent.is_some()
    }

    /// The node's sandbox root certificate, baked into browser templates.
    pub fn sandbox_root_pem(&self) -> String {
        self.root.cert_pem().to_string()
    }

    /// The certificate of the lease's CA, for the sandbox's trust bundle; `None` when the
    /// tool has no secrets and doesn't sign (then nothing is ever intercepted).
    pub fn lease_ca_pem(&self, lease: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .cas
            .get(lease)
            .map(|c| c.cert_pem().to_string())
    }

    /// The owner said no: the card closes and the host stays refused.
    pub fn deny(&self, request: &str) {
        self.state.lock().unwrap().pending.remove(request);
    }

    pub fn log(&self) -> Vec<LogEntry> {
        self.state.lock().unwrap().log.clone()
    }

    /// The manifest's rules, the owner's tool-wide approvals, or a one-time approval.
    pub(super) fn allowed(
        &self,
        policy: &LeasePolicy,
        host: &str,
        port: u16,
        method: &str,
    ) -> bool {
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

    pub(super) fn rewrite_headers(
        &self,
        policy: &LeasePolicy,
        host: &str,
        authority: &str,
        hdrs: &[(String, Vec<u8>)],
    ) -> Result<Vec<(String, Vec<u8>)>, Decision> {
        let signer = self
            .config
            .signed_agent
            .as_ref()
            .filter(|_| policy.sign_requests);
        let mut out = Vec::with_capacity(hdrs.len() + 3);
        for (name, value) in hdrs {
            let lname = name.to_ascii_lowercase();
            if lname.starts_with("proxy-") || lname == "connection" || lname == "keep-alive" {
                continue;
            }
            // Only the proxy signs: whatever the sandbox put there goes.
            if signer.is_some() && lname.starts_with("signature") {
                continue;
            }
            out.push((name.clone(), self.inject(policy, host, value)?));
        }
        if let Some(agent) = signer {
            let now = (now_ms() / 1000) as u64;
            for (n, v) in agent.sign_request(authority, now) {
                out.push((n, v.into_bytes()));
            }
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

    pub(super) fn inject(
        &self,
        policy: &LeasePolicy,
        host: &str,
        value: &[u8],
    ) -> Result<Vec<u8>, Decision> {
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
}

mod approvals;
mod http;
mod transport;
