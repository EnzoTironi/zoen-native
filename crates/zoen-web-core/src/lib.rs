//! Browser boundary. Device signing keys belong to nonexportable WebCrypto, never Rust.

mod client;
mod files;
mod link;
mod living_mutations;
mod living_space;
mod pages;
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
        living_space_enabled: bool,
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
                living_space_enabled: false,
            })
        }
        pub fn set_living_space_enabled(&mut self, enabled: bool) {
            self.living_space_enabled = enabled;
        }
        pub fn living_space_command(
            &self,
            space: &str,
            command_json: &str,
        ) -> Result<String, JsValue> {
            if !self.living_space_enabled {
                return Err(error("Living Space experiment is disabled"));
            }
            let command: serde_json::Value = serde_json::from_str(command_json).map_err(error)?;
            if command["action"] != "home" {
                return Err(error("Living Space browser mutations are not implemented"));
            }
            serde_json::to_string(&self.client.living_space_home(space).map_err(error)?)
                .map_err(error)
        }
        pub fn living_space_mutate(
            &mut self,
            space: &str,
            command_json: &str,
            now_ms: f64,
            edit_id: &str,
        ) -> Result<String, JsValue> {
            if !self.living_space_enabled {
                return Err(error("Living Space experiment is disabled"));
            }
            serde_json::to_string(
                &self
                    .client
                    .living_space_mutate(space, command_json, time(now_ms)?, edit_id)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn file_manifest(&self, item: &str) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.file_manifest(item).map_err(error)?).map_err(error)
        }
        pub fn file_open(&self, item: &str, packed: &[u8]) -> Result<Vec<u8>, JsValue> {
            self.client.file_open(item, packed).map_err(error)
        }
        pub fn file_upload_headers(&self, blob: &str, now_ms: f64) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .file_upload_headers(blob, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn file_prepare(
            &self,
            space: &str,
            name: &str,
            mime: &str,
            bytes: &[u8],
            now_ms: f64,
        ) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .file_prepare(space, name, mime, bytes, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn file_commit(
            &mut self,
            space: &str,
            plan_json: &str,
            now_ms: f64,
            edit_id: &str,
        ) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .file_commit(space, plan_json, time(now_ms)?, edit_id)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn page_create(
            &mut self,
            space: &str,
            title: &str,
            now_ms: f64,
            edit_id: &str,
        ) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .page_create(space, title, time(now_ms)?, edit_id)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn page_download_manifest(&self, item: &str) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.page_download_manifest(item).map_err(error)?)
                .map_err(error)
        }
        pub fn page_blob_supply(
            &mut self,
            item: &str,
            blob: &str,
            sealed: &[u8],
        ) -> Result<(), JsValue> {
            self.client
                .page_blob_supply(item, blob, sealed)
                .map_err(error)
        }
        pub fn page(&self, item: &str) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.page(item).map_err(error)?).map_err(error)
        }
        pub fn page_apply(&mut self, request_json: &str, now_ms: f64) -> Result<String, JsValue> {
            serde_json::to_string(
                &self
                    .client
                    .page_apply(request_json, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
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
            now_ms: f64,
        ) -> Result<String, JsValue> {
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
            let note = serde_json::from_str(note).map_err(error)?;
            serde_json::to_string(
                &self
                    .client
                    .queue_annotation(space, target, note, time(now_ms)?)
                    .map_err(error)?,
            )
            .map_err(error)
        }
        pub fn mark_read(&mut self, space: &str) -> Result<String, JsValue> {
            serde_json::to_string(&self.client.mark_read(space).map_err(error)?).map_err(error)
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
