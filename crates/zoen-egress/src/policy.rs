//! What a tool may reach, declared in its manifest, and the addresses nobody may reach.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Ports every rule gets unless it lists its own.
pub const DEFAULT_PORTS: [u16; 2] = [443, 80];
/// Mail submission is never proxied, whatever a manifest says.
pub const BLOCKED_PORTS: [u16; 3] = [25, 465, 587];

/// One allowlisted destination: an exact host or `*.example.com` (subdomains only).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EgressRule {
    pub host: String,
    #[serde(default = "default_ports")]
    pub ports: Vec<u16>,
    /// Empty means any method.
    #[serde(default)]
    pub methods: Vec<String>,
}

fn default_ports() -> Vec<u16> {
    DEFAULT_PORTS.to_vec()
}

impl EgressRule {
    pub fn host(host: &str) -> Self {
        EgressRule {
            host: host.to_ascii_lowercase(),
            ports: default_ports(),
            methods: vec![],
        }
    }

    pub fn matches(&self, host: &str, port: u16, method: &str) -> bool {
        host_matches(&self.host, host)
            && self.ports.contains(&port)
            && (self.methods.is_empty()
                || self.methods.iter().any(|m| m.eq_ignore_ascii_case(method)))
    }

    /// Rejects IP literals, bare wildcards and empty hosts.
    pub fn validate(&self) -> Result<(), String> {
        let h = self.host.strip_prefix("*.").unwrap_or(&self.host);
        if h.is_empty() || h.contains('*') || !h.contains('.') {
            return Err(format!("egress host `{}` must be a domain name", self.host));
        }
        if h.parse::<IpAddr>().is_ok() || h.starts_with('[') {
            return Err(format!("egress host `{}` is an IP address", self.host));
        }
        if self.ports.is_empty() {
            return Err(format!("egress host `{}` lists no ports", self.host));
        }
        Ok(())
    }
}

/// A named secret the proxy may put into requests to these hosts, and only these.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecretBinding {
    pub name: String,
    pub hosts: Vec<String>,
}

impl SecretBinding {
    pub fn allows(&self, host: &str) -> bool {
        self.hosts.iter().any(|p| host_matches(p, host))
    }
}

/// What code and models see instead of a secret.
pub fn placeholder(name: &str) -> String {
    format!("{PLACEHOLDER_PREFIX}{name}")
}
pub const PLACEHOLDER_PREFIX: &str = "zoen-secret://";

/// `pattern` is a host or `*.domain`; comparison ignores case and a trailing dot.
pub fn host_matches(pattern: &str, host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let pattern = pattern.trim_end_matches('.').to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(base) => host.len() > base.len() + 1 && host.ends_with(&format!(".{base}")),
        None => host == pattern,
    }
}

/// Addresses no sandbox may reach, even through an allowlisted name: loopback, private,
/// link-local (cloud metadata lives at 169.254.169.254), carrier-grade NAT, multicast and
/// unspecified, in IPv4 and IPv6 (including IPv4-mapped).
pub fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return forbidden_v4(v4);
            }
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg0 & 0xfe00) == 0xfc00 // unique local
                || (seg0 & 0xffc0) == 0xfe80 // link-local
                || v6 == Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254) // AWS IMDS v6
        }
    }
}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15
        || o[0] >= 240
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_cover_subdomains_only() {
        assert!(host_matches("*.pypi.org", "files.pypi.org"));
        assert!(!host_matches("*.pypi.org", "pypi.org"));
        assert!(!host_matches("*.pypi.org", "evilpypi.org"));
        assert!(host_matches("API.github.com", "api.github.com."));
    }

    #[test]
    fn metadata_private_and_mapped_addresses_are_forbidden() {
        for a in [
            "169.254.169.254",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:10.0.0.1",
            "0.0.0.0",
        ] {
            assert!(is_forbidden(a.parse().unwrap()), "{a}");
        }
        assert!(!is_forbidden("140.82.112.3".parse().unwrap()));
    }

    #[test]
    fn rules_refuse_ip_literals_and_bare_wildcards() {
        assert!(EgressRule::host("10.0.0.1").validate().is_err());
        assert!(EgressRule::host("*").validate().is_err());
        assert!(EgressRule::host("*.com").validate().is_err());
        assert!(EgressRule::host("api.github.com").validate().is_ok());
    }
}
