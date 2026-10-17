//! Browser boundary. Device signing keys belong to nonexportable WebCrypto, never Rust.

mod client;
mod link;
pub mod media;
mod wire;

pub use client::{Client, Update};
pub use link::{complete_link, create_link, LinkedAccount, PendingLink};
pub use wire::{decode_server, hello, request, ServerMessage};

#[cfg(target_arch = "wasm32")]
mod browser {
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
                || !roda_log::verify_sig(&self.public, payload, &hex::encode(&signature))
            {
                return Err(SignerError::InvalidSignature);
            }
            Ok(signature)
        }
        fn signature_scheme(&self) -> openmls_traits::types::SignatureScheme {
            roda_mls::SUITE.signature_algorithm()
        }
    }

    fn time(value: f64) -> Result<i64, JsValue> {
        if !value.is_finite()
            || value.fract() != 0.0
            || !(0.0..=281_474_976_710_655.0).contains(&value)
        {
            return Err(error("invalid browser timestamp"));
        }
        Ok(value as i64)
    }

    #[wasm_bindgen]
    pub struct BrowserCore {
        client: super::Client<BrowserSigner>,
    }

    #[wasm_bindgen]
    impl BrowserCore {
        #[wasm_bindgen(constructor)]
        pub fn new(account: &str, snapshot: &str) -> Result<BrowserCore, JsValue> {
            let account: super::LinkedAccount = serde_json::from_str(account).map_err(error)?;
            let signer = BrowserSigner {
                public: account.device.clone(),
            };
            Ok(Self {
                client: super::Client::new(account, signer, snapshot).map_err(error)?,
            })
        }
        pub fn bootstrap(&mut self) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.bootstrap().map_err(error)?).map_err(error)
        }
        pub fn receive(&mut self, bytes: &[u8], now_ms: f64) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.receive(bytes, time(now_ms)?).map_err(error)?)
                .map_err(error)
        }
        pub fn queue_message(
            &mut self,
            space: &str,
            text: &str,
            now_ms: f64,
        ) -> Result<String, JsValue> {
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
            also_in_chat: bool,
            now_ms: f64,
        ) -> Result<String, JsValue> {
            let reply = reply_to.filter(|to| !to.is_empty()).map(|to| (to, thread));
            serde_json::to_string(
                &self
                    .client
                    .queue_post_with_chat(
                        space,
                        text,
                        reply,
                        forwarded,
                        also_in_chat,
                        time(now_ms)?,
                    )
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
            let note = serde_json::from_str(note).map_err(error)?;
            serde_json::to_string(
                &self
                    .client
                    .queue_annotation(space, target, note, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn queue_file(
            &mut self,
            space: &str,
            doc: &str,
            now_ms: f64,
        ) -> Result<String, JsValue> {
            let doc = serde_json::from_str(doc).map_err(error)?;
            serde_json::to_string(
                &self
                    .client
                    .queue_file(space, doc, time(now_ms)?)
                    .map_err(error)?,
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
            self.client.local_chunk(blob)
        }

        pub fn fail_outbox(&mut self, client_id: &str, reason: &str) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .fail_outbox(client_id, reason.to_string())
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn mark_read(&mut self, space: &str) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.mark_read(space).map_err(error)?).map_err(error)
        }
        pub fn set_thread_follow(
            &mut self,
            space: &str,
            root: &str,
            follow: Option<bool>,
            now_ms: f64,
        ) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .set_thread_follow(space, root, follow, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn mark_thread_read(
            &mut self,
            space: &str,
            root: &str,
            now_ms: f64,
        ) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .mark_thread_read(space, root, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn queue_join(
            &mut self,
            space: &str,
            role: &str,
            code: &str,
            now_ms: f64,
        ) -> Result<String, JsValue> {
            let role = serde_json::from_value(serde_json::Value::String(role.to_string()))
                .map_err(error)?;
            serde_json::to_string(
                &self
                    .client
                    .queue_join(space, role, code, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn view(&self) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.conversations()).map_err(error)
        }
    }

    fn error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    /// `{doc, uploads:[{blob, sealed}]}` for a file the host will upload before queueing.
    #[wasm_bindgen]
    pub fn seal_file(
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumb: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
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
        roda_proto::blob_put_message(sha, ts_ms as i64, relay)
    }

    #[wasm_bindgen]
    pub fn create_link(device: &str, relay: &str) -> Result<String, JsValue> {
        serde_json::to_string(&super::create_link(device, relay).map_err(error)?).map_err(error)
    }

    #[wasm_bindgen]
    pub fn complete_link(pending: &str, sealed: &[u8]) -> Result<String, JsValue> {
        let pending = serde_json::from_str(pending).map_err(error)?;
        serde_json::to_string(&super::complete_link(&pending, sealed).map_err(error)?)
            .map_err(error)
    }

    #[wasm_bindgen]
    pub fn hello(identity: &str, device: &str, cert: &str) -> Vec<u8> {
        super::hello(identity, device, cert)
    }

    #[wasm_bindgen]
    pub fn auth_message(nonce: &str, relay: &str) -> Vec<u8> {
        roda_proto::auth_message(nonce, relay)
    }

    #[wasm_bindgen]
    pub fn device_cert_message(device: &str) -> Vec<u8> {
        roda_log::device_cert_message(device)
    }

    #[wasm_bindgen]
    pub fn auth(signature: &str) -> Vec<u8> {
        roda_proto::ClientFrame::Auth {
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
}
