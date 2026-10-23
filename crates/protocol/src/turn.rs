//! Short-lived TURN transport credentials; never authority to join a private call.
pub const CREDENTIAL_TTL_SECONDS: i64 = 120;
pub const MAX_URLS: usize = 4;

#[derive(Clone, PartialEq, Eq)]
pub struct TurnCredentials {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
    pub expires_at_ms: i64,
}

impl std::fmt::Debug for TurnCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnCredentials")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish_non_exhaustive()
    }
}

pub fn valid_url(url: &str) -> bool {
    if url.len() > 320 || !url.is_ascii() {
        return false;
    }
    let (tls, rest) = if let Some(rest) = url.strip_prefix("turns:") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("turn:") {
        (false, rest)
    } else {
        return false;
    };
    let authority = match rest.split_once('?') {
        Some((host, "transport=tcp")) => host,
        Some((host, "transport=udp")) if !tls => host,
        Some(_) => return false,
        None => rest,
    };
    let host = if authority.starts_with('[') {
        let Some((host, tail)) = authority.split_once(']') else {
            return false;
        };
        if host[1..].parse::<std::net::Ipv6Addr>().is_err() || !valid_port_suffix(tail) {
            return false;
        }
        return true;
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if !valid_port_suffix(&format!(":{port}")) {
            return false;
        }
        host
    } else {
        authority
    };
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn valid_port_suffix(value: &str) -> bool {
    value.is_empty()
        || value.strip_prefix(':').is_some_and(|port| {
            !port.is_empty()
                && port.bytes().all(|c| c.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|n| n > 0)
        })
}

pub fn valid_urls(urls: &[String]) -> bool {
    !urls.is_empty()
        && urls.len() <= MAX_URLS
        && urls.iter().all(|url| valid_url(url))
        && urls.iter().any(|url| url.starts_with("turns:"))
}

impl TurnCredentials {
    pub fn valid(&self) -> bool {
        valid_urls(&self.urls)
            && self.username.len() <= 128
            && self.credential.len() == 28
            && self.credential.strip_suffix('=').is_some_and(|body| {
                body.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"+/".contains(&c))
            })
            && self
                .username
                .split_once(':')
                .is_some_and(|(expiry, opaque)| {
                    expiry.parse::<i64>().ok().and_then(|n| n.checked_mul(1000))
                        == Some(self.expires_at_ms)
                        && self.expires_at_ms > 0
                        && opaque.len() == 64
                        && opaque
                            .bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                })
    }
}

#[cfg(test)]
mod tests;
