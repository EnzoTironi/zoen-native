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
//! P0 scope: HTTP CONNECT tunnels and plain-HTTP forwarding. Secrets are injected into
//! plain-HTTP request headers; HTTPS injection needs the per-lease CA of phase 1.

pub mod policy;
pub mod proxy;

pub use policy::{
    host_matches, is_forbidden, placeholder, EgressRule, SecretBinding, PLACEHOLDER_PREFIX,
};
pub use proxy::{
    ApprovalScope, ApprovalSink, Decision, Egress, EgressConfig, LeasePolicy, LogEntry, Resolver,
    SecretSource,
};
