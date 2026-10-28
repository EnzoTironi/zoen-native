//! Existing complete domain bindings on the same BrowserCore.
use super::*;

#[wasm_bindgen]
impl BrowserCore {
    pub fn integrity_request(&self, request: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.integrity_request(request).map_err(error)?)
            .map_err(error)
    }
    pub fn membership_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .membership_request(request, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn document_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .document_request(request, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn agent_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .agent_request(request, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn profile_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .profile_request(request, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn profile_save(
        &mut self,
        request: &str,
        bytes: Vec<u8>,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .profile_save(request, bytes, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn profile_photo_framed(&self, identity: &str, expected: &str) -> Result<Vec<u8>, JsValue> {
        let _language = localization::enter(self.lang);
        self.client
            .profile_photo_framed(identity, expected)
            .map_err(error)
    }
    pub fn profile_photo_received(
        &mut self,
        identity: &str,
        expected: &str,
        sealed: Vec<u8>,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .profile_photo_received(identity, expected, &sealed)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn handle_profile(&self, handle: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        self.client.handle_profile(handle).map_err(error)
    }
    pub fn handle_confirmed(&mut self, profile: &str, expected: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .handle_confirmed(profile, expected)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn link_prepare(
        &mut self,
        primary: &BrowserPrimary,
        code: &str,
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .link_prepare(&primary.primary, code, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn link_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .link_request(request, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn history_status(&self) -> String {
        let _language = localization::enter(self.lang);
        self.client.history_status()
    }
    pub fn history_start(&mut self, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.history_start(time(now_ms)?).map_err(error)?)
            .map_err(error)
    }
    pub fn history_manifest(&mut self, sealed: &[u8], now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .history_manifest(sealed, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn history_chunk(
        &mut self,
        index: u32,
        sealed: &[u8],
        now_ms: f64,
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .history_chunk(index, sealed, time(now_ms)?)
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn history_finish(&mut self, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.history_finish(time(now_ms)?).map_err(error)?)
            .map_err(error)
    }
    pub fn history_cleaned(&mut self, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.history_cleaned(time(now_ms)?).map_err(error)?)
            .map_err(error)
    }
    pub fn directory_profiles(&mut self, profiles: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(&self.client.directory_profiles(profiles).map_err(error)?)
            .map_err(error)
    }
    pub fn typing_frame(&self, space: &str, typing: bool) -> Result<Vec<u8>, JsValue> {
        let _language = localization::enter(self.lang);
        self.client.typing_frame(space, typing).map_err(error)
    }
    pub fn app_request(&mut self, request: &str, now_ms: f64) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        serde_json::to_string(
            &self
                .client
                .app_request(request, time(now_ms)?, self.lang.tag())
                .map_err(error)?,
        )
        .map_err(error)
    }
    pub fn document_uploads(&self) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        self.client.document_uploads().map_err(error)
    }
    pub fn document_uploaded(&mut self, blobs: &str) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let blobs = serde_json::from_str::<Vec<String>>(blobs).map_err(error)?;
        serde_json::to_string(&self.client.document_uploaded(&blobs).map_err(error)?).map_err(error)
    }
    pub fn document_cache_blob(
        &mut self,
        reference: &str,
        bytes: &[u8],
    ) -> Result<String, JsValue> {
        let _language = localization::enter(self.lang);
        let reference = serde_json::from_str(reference).map_err(error)?;
        serde_json::to_string(
            &self
                .client
                .document_cache_blob(reference, bytes.to_vec())
                .map_err(error)?,
        )
        .map_err(error)
    }
}
