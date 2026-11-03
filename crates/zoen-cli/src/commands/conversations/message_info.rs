//! Exact account/message scope through the canonical read-only Core command.
use crate::Client;
use client_core::MessageInfoScope;

pub(super) async fn read(client: &Client, space: &str, message: &str) -> Result<String, String> {
    let account = client
        .account()
        .filter(|a| a.unlocked && !a.setup_pending)
        .ok_or("Message info requires an unlocked account")?;
    let scope = MessageInfoScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.to_owned(),
        message_id: message.to_owned(),
    };
    client
        .client_request(serde_json::json!({"op":"message_info","scope":scope}).to_string())
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
