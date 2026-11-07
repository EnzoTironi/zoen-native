//! Shared bounded synchronous decision transport; callers choose their Gate policy.
use gate::HttpTransport;
use std::time::Duration;
const GATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const GATE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GATE_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub(crate) struct ReqwestTransport {
    client: reqwest::Client,
    max_response_bytes: usize,
}
impl ReqwestTransport {
    pub(crate) fn new() -> Result<Self, String> {
        Self::with_limits(
            reqwest::Client::builder(),
            GATE_REQUEST_TIMEOUT,
            GATE_CONNECT_TIMEOUT,
            GATE_MAX_RESPONSE_BYTES,
        )
    }

    pub(crate) fn with_limits(
        builder: reqwest::ClientBuilder,
        timeout: Duration,
        connect_timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, String> {
        let client = builder
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
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
                if !response.status().is_success() {
                    return Err("status".to_string());
                }
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

#[cfg(test)]
#[path = "gate_transport_tests.rs"]
mod tests;
