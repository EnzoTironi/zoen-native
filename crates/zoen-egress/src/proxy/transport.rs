use super::http::{authority, forward, parse_target, read_head, request_head, Head, Prefixed};
use super::Egress;
use super::{
    ct_eq, now_ms, Arc, Ctx, Decision, Duration, Instant, LogEntry, TcpListener, BLOCKED_PORTS,
};
use base64::Engine;
use std::net::IpAddr;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
impl Egress {
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
            let id = match self.open_card(&policy, &target, &method) {
                Ok(id) => id,
                Err(decision) => return self.refuse(&mut conn, &ctx, decision, None).await,
            };
            return self
                .refuse(&mut conn, &ctx, Decision::NeedsApproval, Some(&id))
                .await;
        }

        // Secrets: placeholders become values only for hosts the secret is bound to.
        let mut out_headers = vec![];
        if target.path.is_some() {
            let default_port = if target.tls { 443 } else { 80 };
            let authority = authority(&target.host, target.port, default_port);
            match self.rewrite_headers(&policy, &target.host, &authority, &hdrs) {
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
            let intercept = ca.filter(|c| c.covers(&target.host));
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
                let id = match self.open_card(&policy, &target, &method) {
                    Ok(id) => id,
                    Err(decision) => return self.refuse(&mut inner, &ctx, decision, None).await,
                };
                return self
                    .refuse(&mut inner, &ctx, Decision::NeedsApproval, Some(&id))
                    .await;
            }
            let authority = authority(&target.host, target.port, 443);
            let out_headers = match self.rewrite_headers(&policy, &target.host, &authority, &hdrs) {
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
}
