use super::{Decision, Target, BLOCKED_PORTS};
use std::net::IpAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) enum Head {
    Closed,
    TooBig,
    Complete(Vec<u8>, usize),
}

pub(super) async fn read_head<S: AsyncRead + Unpin>(
    conn: &mut S,
    max: usize,
) -> std::io::Result<Head> {
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

pub(super) struct ParsedHead {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: Vec<(String, Vec<u8>)>,
}

pub(super) fn parse_head(bytes: &[u8]) -> Option<ParsedHead> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    if !matches!(request.parse(bytes), Ok(httparse::Status::Complete(_))) {
        return None;
    }
    Some(ParsedHead {
        method: request.method?.to_owned(),
        path: request.path?.to_owned(),
        headers: request
            .headers
            .iter()
            .map(|header| (header.name.to_owned(), header.value.to_vec()))
            .collect(),
    })
}

/// One request upstream: `Connection: close`, so one request per connection.
pub(super) fn request_head(method: &str, path: &str, headers: &[(String, Vec<u8>)]) -> Vec<u8> {
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

pub(super) async fn forward<C, U>(
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
    let (a, b) = tokio::io::copy_bidirectional(conn, upstream).await?;
    let prefix = head
        .len()
        .checked_add(leftover.len())
        .ok_or_else(|| std::io::Error::other("Transfer prefix overflow"))?;
    let prefix = u64::try_from(prefix).map_err(std::io::Error::other)?;
    let up = prefix
        .checked_add(a)
        .ok_or_else(|| std::io::Error::other("Transfer byte count overflow"))?;
    Ok((up, b))
}

/// A stream with bytes already read in front of it (the TLS `ClientHello` can arrive together
/// with the CONNECT head).
pub(super) struct Prefixed<S> {
    pub(super) prefix: Vec<u8>,
    pub(super) inner: S,
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

/// `@authority`: the host, plus the port when it isn't the scheme's default.
pub(super) fn authority(host: &str, port: u16, default_port: u16) -> String {
    if port == default_port {
        host.to_ascii_lowercase()
    } else {
        format!("{}:{port}", host.to_ascii_lowercase())
    }
}

pub(super) fn split_host_port(authority: &str, default_port: u16) -> Option<(String, u16)> {
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

pub(super) fn validated_target(method: &str, raw: &str) -> Result<Target, Decision> {
    let target = parse_target(method, raw).ok_or(Decision::BadRequest)?;
    if target
        .host
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok()
    {
        return Err(Decision::IpLiteral);
    }
    if BLOCKED_PORTS.contains(&target.port) {
        return Err(Decision::BlockedPort);
    }
    Ok(target)
}

pub(super) fn parse_target(method: &str, raw: &str) -> Option<Target> {
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
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
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

    struct BrokenRead;
    impl AsyncRead for BrokenRead {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected upstream read failure",
            )))
        }
    }
    impl AsyncWrite for BrokenRead {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
            buffer: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Ok(buffer.len()))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn forwarding_io_failure_is_not_a_successful_transfer() {
        let (mut connection, _peer) = tokio::io::duplex(64);
        let result = forward(&mut connection, &mut BrokenRead, b"synthetic-head", b"").await;
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
    }

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
