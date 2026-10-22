mod domains;

use openmls_traits::signatures::{Signer, SignerError};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = globalThis, js_name = zoenSign)]
    fn sign_device(payload: &[u8]) -> Result<js_sys::Uint8Array, JsValue>;
}

struct BrowserSigner {
    public: String,
}
impl Signer for BrowserSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        let signature = sign_device(payload)
            .map_err(|_| SignerError::SigningError)?
            .to_vec();
        if signature.len() != 64
            || !event_log::verify_sig(&self.public, payload, &hex::encode(&signature))
        {
            return Err(SignerError::InvalidSignature);
        }
        Ok(signature)
    }
    fn signature_scheme(&self) -> openmls_traits::types::SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}

fn time(value: f64) -> Result<i64, JsValue> {
    if !value.is_finite() || value.fract() != 0.0 || !(0.0..=281_474_976_710_655.0).contains(&value)
    {
        return Err(error("invalid browser timestamp"));
    }
    Ok(value as i64)
}

fn session_language(locale: &str) -> Result<localization::Lang, JsValue> {
    match locale {
        "en" => Ok(localization::Lang::En),
        "pt-BR" => Ok(localization::Lang::Pt),
        _ => Err(error("invalid browser session language")),
    }
}

#[wasm_bindgen]
pub struct BrowserPrimary {
    primary: super::Primary,
    lang: localization::Lang,
}
#[wasm_bindgen]
impl BrowserPrimary {
    #[wasm_bindgen(constructor)]
    pub fn new(
        device: &str,
        relay: &str,
        name: &str,
        locale: &str,
    ) -> Result<BrowserPrimary, JsValue> {
        let lang = session_language(locale)?;
        let _language = localization::enter(lang);
        Ok(Self {
            primary: super::Primary::new(device, relay, name).map_err(error)?,
            lang,
        })
    }
    pub fn restore(
        seed: &[u8],
        account: &str,
        device: &str,
        locale: &str,
    ) -> Result<BrowserPrimary, JsValue> {
        let lang = session_language(locale)?;
        let _language = localization::enter(lang);
        Ok(Self {
            primary: super::Primary::restore(
                seed,
                serde_json::from_str(account).map_err(error)?,
                device,
            )
            .map_err(error)?,
            lang,
        })
    }
    pub fn account(&self) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(self.primary.account()).map_err(error)
    }
    pub fn authority_bytes(&self) -> Vec<u8> {
        let _language = localization::enter(self.lang);
        self.primary.authority_bytes()
    }
    pub fn bind_message(&self, proof: &str) -> Result<Vec<u8>, JsValue> {
        let _language = localization::enter(self.lang);
        self.primary.binding_message(proof).map_err(error)
    }
    pub fn finish_bind(&self, proof: &str, signature: &[u8]) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .primary
                .finish_binding(proof, signature)
                .map_err(error)?,
        )
        .map_err(error)
    }
}

#[wasm_bindgen]
pub struct BrowserCore {
    client: super::Client<BrowserSigner>,
    lang: localization::Lang,
}

#[wasm_bindgen]
impl BrowserCore {
    #[wasm_bindgen(constructor)]
    pub fn new(account: &str, snapshot: &str, locale: &str) -> Result<BrowserCore, JsValue> {
        let lang = session_language(locale)?;
        let _language = localization::enter(lang);
        let account: super::LinkedAccount = serde_json::from_str(account).map_err(error)?;
        let signer = BrowserSigner {
            public: account.device.clone(),
        };
        Ok(Self {
            client: super::Client::new(account, signer, snapshot).map_err(error)?,
            lang,
        })
    }
    pub fn bootstrap(&mut self) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.bootstrap().map_err(error)?).map_err(error)
    }
    pub fn receive(&mut self, bytes: &[u8], now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.receive(bytes, time(now_ms)?).map_err(error)?)
            .map_err(error)
    }
    pub fn queue_message(
        &mut self,
        space: &str,
        text: &str,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .queue_message(space, text, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn queue_post(
        &mut self,
        space: &str,
        text: &str,
        reply_to: Option<String>,
        thread: bool,
        forwarded: bool,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let reply = reply_to.filter(|to| !to.is_empty()).map(|to| (to, thread));
        serde_json::to_string(
            &self
                .client
                .queue_post(space, text, reply, forwarded, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    /// `note` is the shared Annotation JSON, e.g. {"React":{"emoji":"👍"}} or "Delete".
    pub fn queue_annotation(
        &mut self,
        space: &str,
        target: &str,
        note: &str,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let note = serde_json::from_str(note).map_err(error)?;
        serde_json::to_string(
            &self
                .client
                .queue_annotation(space, target, note, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn queue_file(&mut self, space: &str, doc: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let doc = serde_json::from_str(doc).map_err(error)?;
        serde_json::to_string(
            &self
                .client
                .queue_file(space, doc, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    /// One activation's explicit producer/sequence/acknowledgement lifecycle.
    pub fn attachment_producer(&mut self, request: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        self.client.attachment_producer(request).map_err(error)
    }

    /// Persist exact attachment intent and ciphertext before the first relay PUT.
    pub fn queue_attachment(
        &mut self,
        request: &str,
        framed: &[u8],
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let (update, item, message) = self
            .client
            .queue_attachment(request, framed, time(now_ms)?)
            .map_err(error)?;
        serde_json::to_string(
            &serde_json::json!({"update":update,"result":{"itemId":item,"messageId":message}}),
        )
        .map_err(error)
    }

    /// An ordered attachment selection crosses the boundary as bounded binary frames.
    pub fn queue_attachment_batch(
        &mut self,
        request: &str,
        framed: &[u8],
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let (update, entries) = self
            .client
            .queue_attachment_batch(request, framed, time(now_ms)?)
            .map_err(error)?;
        let result = entries
            .into_iter()
            .map(|(item, message)| serde_json::json!({"itemId":item,"messageId":message}))
            .collect::<Vec<_>>();
        serde_json::to_string(&serde_json::json!({"update":update,"result":result})).map_err(error)
    }

    pub fn queue_file_record(
        &mut self,
        space: &str,
        doc: &str,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let doc = serde_json::from_str(doc).map_err(error)?;
        let (update, item, message) = self
            .client
            .queue_file_record(space, doc, time(now_ms)?)
            .map_err(error)?;
        serde_json::to_string(
            &serde_json::json!({"update":update,"result":{"itemId":item,"messageId":message}}),
        )
        .map_err(error)
    }
    pub fn queue_failed_file(
        &mut self,
        space: &str,
        doc: &str,
        now_ms: f64,
        reason: &str,
        chunks: &str,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let doc = serde_json::from_str(doc).map_err(error)?;
        let chunks = serde_json::from_str(chunks).map_err(error)?;
        serde_json::to_string(
            &self
                .client
                .queue_failed_file(space, doc, time(now_ms)?, reason.into(), chunks)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn local_chunk(&self, blob: &str) -> Vec<u8> {
        let _language = localization::enter(self.lang);
        self.client.local_chunk(blob)
    }

    pub fn fail_outbox(&mut self, client_id: &str, reason: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .fail_outbox(client_id, reason.to_string())
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn mark_read(&mut self, space: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.mark_read(space).map_err(error)?).map_err(error)
    }
    pub fn queue_join(
        &mut self,
        space: &str,
        role: &str,
        code: &str,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let role =
            serde_json::from_value(serde_json::Value::String(role.to_string())).map_err(error)?;
        serde_json::to_string(
            &self
                .client
                .queue_join(space, role, code, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn view(&self) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.conversations()).map_err(error)
    }
    pub fn claim_clock(&mut self, value: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        self.client.claim_clock(value).map_err(error)?;
        Ok(String::new())
    }
    pub fn maintenance(&mut self, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.maintenance(time(now_ms)?).map_err(error)?)
            .map_err(error)
    }
}

fn error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// `{doc, uploads:[{blob, sealed}]}` for a file the host will upload before queueing.
#[wasm_bindgen]
pub fn seal_file(name: &str, mime: &str, bytes: &[u8], thumb: &[u8]) -> Result<Vec<u8>, JsValue> {
    let thumb = (!thumb.is_empty()).then_some(thumb);
    let sealed = super::media::seal_file(name, mime, bytes, thumb).map_err(error)?;
    let uploads: Vec<_> = sealed
        .uploads
        .iter()
        .map(|u| serde_json::json!({"blob": u.blob, "len": u.sealed.len()}))
        .collect();
    let head = serde_json::to_vec(&serde_json::json!({"doc": sealed.doc, "uploads": uploads}))
        .map_err(error)?;
    let mut out = Vec::with_capacity(4 + head.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for upload in &sealed.uploads {
        out.extend_from_slice(&upload.sealed);
    }
    Ok(out)
}

#[wasm_bindgen]
pub fn open_chunk(chunk: &str, sealed: &[u8]) -> Result<Vec<u8>, JsValue> {
    let chunk = serde_json::from_str(chunk).map_err(error)?;
    super::media::open_chunk(&chunk, sealed).map_err(error)
}

#[wasm_bindgen]
pub fn blob_put_message(sha: &str, ts_ms: f64, relay: &str) -> Vec<u8> {
    protocol::blob_put_message(sha, ts_ms as i64, relay)
}

#[wasm_bindgen]
pub fn create_link(device: &str, relay: &str) -> Result<String, JsValue> {
    serde_json::to_string(&super::create_link(device, relay).map_err(error)?).map_err(error)
}

#[wasm_bindgen]
pub fn complete_link(pending: &str, sealed: &[u8]) -> Result<String, JsValue> {
    let pending = serde_json::from_str(pending).map_err(error)?;
    serde_json::to_string(&super::complete_link(&pending, sealed).map_err(error)?).map_err(error)
}

#[wasm_bindgen]
pub fn hello(identity: &str, device: &str, cert: &str) -> Vec<u8> {
    super::hello(identity, device, cert)
}

#[wasm_bindgen]
pub fn auth_message(nonce: &str, relay: &str) -> Vec<u8> {
    protocol::auth_message(nonce, relay)
}

#[wasm_bindgen]
pub fn device_cert_message(device: &str) -> Vec<u8> {
    event_log::device_cert_message(device)
}

#[wasm_bindgen]
pub fn ping() -> Vec<u8> {
    protocol::ClientFrame::Ping.encode()
}

#[wasm_bindgen]
pub fn auth(signature: &str) -> Vec<u8> {
    protocol::ClientFrame::Auth {
        sig: signature.to_string(),
    }
    .encode()
}

#[wasm_bindgen]
pub fn request(id: u32, kind: &str, value: &str) -> Result<Vec<u8>, JsValue> {
    super::request(id.into(), kind, value).map_err(error)
}

#[wasm_bindgen]
pub fn decode_server(bytes: &[u8]) -> Result<String, JsValue> {
    serde_json::to_string(&super::decode_server(bytes).map_err(error)?).map_err(error)
}
