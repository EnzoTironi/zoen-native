//! Existing provider construction and bounded HTTP transport for the local host.

use super::*;

impl ReqwestTransport {
    pub(super) fn new() -> Result<Self, String> {
        Self::with_limits(
            reqwest::Client::builder(),
            GATE_REQUEST_TIMEOUT,
            GATE_CONNECT_TIMEOUT,
            GATE_MAX_RESPONSE_BYTES,
        )
    }

    pub(super) fn with_limits(
        builder: reqwest::ClientBuilder,
        timeout: Duration,
        connect_timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, String> {
        let client = builder
            .connect_timeout(connect_timeout)
            .read_timeout(timeout)
            .timeout(timeout)
            .build()
            .map_err(|_| "transport configuration".to_string())?;
        Ok(Self {
            client,
            max_response_bytes,
        })
    }
}

impl HttpTransport for ReqwestTransport {
    fn post_json(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<String, String> {
        let mut rq = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(b) = bearer {
            rq = rq.bearer_auth(b);
        }
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut response = rq.send().await.map_err(|_| "transport".to_string())?;
                if response
                    .content_length()
                    .is_some_and(|length| length > self.max_response_bytes as u64)
                {
                    return Err("response too large".to_string());
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|_| "body".to_string())? {
                    if chunk.len() > self.max_response_bytes.saturating_sub(bytes.len()) {
                        return Err("response too large".to_string());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                // Retain reqwest::Response::text's UTF-8/lossy behavior with
                // this workspace's charset-disabled reqwest configuration.
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            })
        })
    }
}

/// Clef (clef-flash) on Workers AI. Without credentials the runtime refuses to start
/// unless `ZOEN_AGENT_GATE=offline` explicitly accepts the hard rules + offline floor.
pub(super) fn gate() -> Result<Gate, String> {
    if std::env::var("ZOEN_AGENT_GATE").as_deref() == Ok("offline") {
        return Ok(Gate::default());
    }
    match (env("CLOUDFLARE_WORKERS_AI_TOKEN"), env("CLOUDFLARE_ACCOUNT_ID")) {
        (Some(token), Some(account)) => Ok(Gate {
            remote: Some(Box::new(Clef {
                transport: Arc::new(ReqwestTransport::new()?),
                account,
                token,
                model: ClefModel::Flash,
            })),
            ..Gate::default()
        }),
        _ => Err(
            "no gate: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID, or ZOEN_AGENT_GATE=offline"
                .into(),
        ),
    }
}

pub(super) fn brain() -> Result<Box<dyn Brain>, String> {
    if let Some(scripted) = scripted_brain_from_env() {
        return scripted;
    }
    match env("ZOEN_BRAIN").as_deref() {
        Some("codex-local") => CodexBrain::from_env().map(|b| Box::new(b) as Box<dyn Brain>),
        None | Some("workers-ai") => ModelBrain::workers_ai_from_env()
            .map(|b| Box::new(b) as Box<dyn Brain>)
            .ok_or_else(|| {
                "no model: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID".into()
            }),
        Some(_) => Err("unsupported ZOEN_BRAIN; use workers-ai or codex-local".into()),
    }
}
