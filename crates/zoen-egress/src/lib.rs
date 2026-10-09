//! # zoen-egress
//!
//! The only way out of a Zoen sandbox (ADR 0028, section 6). One proxy per sandbox node; the
//! VMs' network allows nothing else. For each lease it:
//!
//! 1. allows only the hosts the tool's manifest lists, resolves names itself and refuses IP
//!    literals and private, loopback, link-local and metadata addresses;
//! 2. swaps secret placeholders (`zoen-secret://github`) for the real value only on requests
//!    to the hosts that secret is bound to, so code and models never see a secret;
//! 3. turns a request to an unlisted host into an approval card (`AgentRequest`) and refuses
//!    it with `EGRESS_NEEDS_APPROVAL` until the owner says yes;
//! 4. logs metadata only: lease, tool, host, port, method, decision, bytes, duration.
//!
//! Transports: HTTP CONNECT (an opaque tunnel, or intercepted with the lease's own CA when
//! one of its secrets is bound to that host, see [`tls`]), plain HTTP, and absolute-form
//! `https://` requests (the proxy opens the TLS connection). Secrets go into request headers.
//!
//! Browser leases also get their requests signed with Web Bot Auth ([`webbotauth`]), and
//! their CA is issued under the node's sandbox root, which browser templates trust
//! ([`tls::SandboxRoot`]).

pub mod policy;
pub mod proxy;
pub mod tls;
pub mod webbotauth;

pub use policy::{
    host_matches, is_forbidden, placeholder, EgressRule, SecretBinding, PLACEHOLDER_PREFIX,
};
pub use proxy::{
    ApprovalScope, ApprovalSink, Decision, Egress, EgressConfig, LeasePolicy, LogEntry, Resolver,
    SecretSource,
};
pub use tls::SandboxRoot;
pub use webbotauth::SignedAgent;
