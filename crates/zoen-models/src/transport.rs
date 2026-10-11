use bytes::Bytes;
use rig_core::http_client::{
    Error, HttpClientExt, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use std::sync::{Arc, Mutex};

use crate::OutputFailure;

#[derive(Default)]
pub(crate) struct Capture {
    pub status: Option<u16>,
    pub request_id: Option<String>,
    pub body: Vec<u8>,
    pub complete: bool,
    pub failure: Option<OutputFailure>,
}

#[derive(Clone)]
pub(crate) struct BoundedHttp {
    pub client: reqwest::Client,
    pub endpoint: String,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub capture: Arc<Mutex<Capture>>,
}

#[derive(Debug, thiserror::Error)]
#[error("bounded provider transport failed")]
struct TransportFailure;

impl BoundedHttp {
    fn fail(&self, failure: OutputFailure) -> Error {
        self.capture.lock().expect("capture lock").failure = Some(failure);
        Error::instance(TransportFailure)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Rig's HttpClientExt boundary requires its owned Error type"
    )]
    async fn unary<U: From<Bytes> + Send + 'static>(
        self,
        req: Request<Bytes>,
    ) -> Result<Response<LazyBody<U>>, Error> {
        // Provider configuration is pinned by the host. Neither model output
        // nor a forwarded SDK request may change its destination or method.
        if req.method() != http::Method::POST
            || req.uri().to_string() != self.endpoint
            || req.body().len() > self.max_request_bytes
        {
            return Err(self.fail(OutputFailure::UnsupportedOutput));
        }
        let (parts, body) = req.into_parts();
        let mut response = self
            .client
            .post(&self.endpoint)
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(|error| {
                self.fail(if error.is_timeout() {
                    OutputFailure::Timeout
                } else {
                    OutputFailure::Transport
                })
            })?;
        let status = response.status();
        let headers = response.headers().clone();
        {
            let mut capture = self.capture.lock().expect("capture lock");
            capture.status = Some(status.as_u16());
            capture.request_id = headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .filter(|value| value.len() <= 256 && !value.chars().any(char::is_control))
                .map(str::to_owned);
        }
        if response
            .content_length()
            .is_some_and(|n| n > self.max_response_bytes as u64)
        {
            return Err(self.fail(OutputFailure::ResponseTooLarge));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            self.capture.lock().expect("capture lock").body = std::mem::take(&mut bytes);
            self.fail(if error.is_timeout() {
                OutputFailure::Timeout
            } else {
                OutputFailure::Transport
            })
        })? {
            if chunk.len() > self.max_response_bytes.saturating_sub(bytes.len()) {
                self.capture.lock().expect("capture lock").body = bytes;
                return Err(self.fail(OutputFailure::ResponseTooLarge));
            }
            bytes.extend_from_slice(&chunk);
        }
        {
            let mut capture = self.capture.lock().expect("capture lock");
            capture.body = bytes.clone();
            capture.complete = true;
        }
        if !status.is_success() {
            self.capture.lock().expect("capture lock").failure =
                Some(OutputFailure::ProviderRejected);
            // Keep sensitive error bytes only in the capture, never in SDK errors.
            return Err(Error::non_success_with_details(
                status,
                http::HeaderMap::new(),
                "provider rejected request".into(),
            ));
        }
        let body: LazyBody<U> = Box::pin(async move { Ok(U::from(Bytes::from(bytes))) });
        let mut output = Response::builder().status(status);
        // The wire only needs content type and request identity. Do not forward
        // cookies, credentials or arbitrary provider headers into SDK diagnostics.
        for name in ["content-type", "x-request-id"] {
            if let Some(value) = headers.get(name) {
                output = output.header(name, value);
            }
        }
        output.body(body).map_err(Error::Protocol)
    }
}

impl HttpClientExt for BoundedHttp {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl std::future::Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        let this = self.clone();
        let (parts, body) = req.into_parts();
        this.unary(Request::from_parts(parts, body.into()))
    }

    fn send_multipart<U>(
        &self,
        _: Request<MultipartForm>,
    ) -> impl std::future::Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        let error = self.fail(OutputFailure::UnsupportedOutput);
        async move { Err(error) }
    }

    fn send_streaming<T>(
        &self,
        _: Request<T>,
    ) -> impl std::future::Future<Output = Result<StreamingResponse, Error>> + Send
    where
        T: Into<Bytes> + Send,
    {
        let error = self.fail(OutputFailure::UnsupportedOutput);
        async move { Err(error) }
    }
}
