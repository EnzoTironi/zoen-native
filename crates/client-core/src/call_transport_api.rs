//! Native RTC transport boundary. No JavaScript bridge exposes SDP/ICE credentials.
use crate::{Client, CoreError};
use types::call_transport::CallTransportSignal;

#[uniffi::export]
impl Client {
    pub fn call_native_state(
        &self,
        identity_id: String,
        device_id: String,
        space_id: String,
    ) -> Result<String, CoreError> {
        let engine = self.lock();
        engine.assert_call_owner(&identity_id, &device_id)?;
        serde_json::to_string(&engine.call_history(&space_id)?).map_err(|_| invalid())
    }

    pub fn call_transport_read(
        &self,
        identity_id: String,
        device_id: String,
        space_id: String,
        call_id: String,
    ) -> Result<String, CoreError> {
        let engine = self.lock();
        let records = engine.call_transport_read(&identity_id, &device_id, &space_id, &call_id)?;
        serde_json::to_string(&records).map_err(|_| invalid())
    }

    pub fn call_transport_send(
        &self,
        identity_id: String,
        device_id: String,
        space_id: String,
        signal_json: String,
    ) -> Result<(), CoreError> {
        if signal_json.len() > 52 * 1024 {
            return Err(invalid());
        }
        let signal: CallTransportSignal =
            serde_json::from_str(&signal_json).map_err(|_| invalid())?;
        self.lock()
            .call_transport_send(&identity_id, &device_id, &space_id, signal)
    }
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid call transport data".into(),
    }
}
