//! An ephemeral execution lease over the existing account and Net owners.
use super::{directory, http_base, invalid, Client, CoreError, Op, Reply};
use crate::{engine::Engine, net::RequestHandle};
use directory::DirectoryOwner;

pub(super) struct LinkOwner {
    owner: DirectoryOwner,
    channel: RequestHandle,
}
impl LinkOwner {
    pub(super) fn capture(client: &Client) -> Result<Self, CoreError> {
        let (owner, channel) = client.device_directory_owner()?;
        Ok(Self { owner, channel })
    }

    pub(super) fn with<T>(
        &self,
        client: &Client,
        apply: impl FnOnce(&mut Engine) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        client.with_device_directory_completion(&self.owner, &self.channel, apply)
    }

    pub(super) async fn request(&self, client: &Client, op: Op) -> Result<Reply, CoreError> {
        self.with(client, |_| Ok(()))?;
        let reply = self.channel.clone().send(op).await.map_err(invalid)?;
        self.with(client, |_| Ok(reply))
    }

    pub(super) fn relay_base(&self, client: &Client) -> Result<String, CoreError> {
        self.with(client, |engine| {
            let account = engine.account().ok_or_else(|| invalid("account"))?;
            Ok(http_base(&account.relay_url))
        })
    }

    pub(super) fn signed_headers(
        &self,
        client: &Client,
        message: impl FnOnce(i64) -> Vec<u8>,
    ) -> Result<(String, String, String), CoreError> {
        self.with(client, |engine| {
            let author = engine
                .net
                .author
                .as_ref()
                .ok_or_else(|| invalid("account"))?;
            let ts = crate::engine::now_ms();
            Ok((
                author.key.id(),
                ts.to_string(),
                author.key.sign(&message(ts)),
            ))
        })
    }
}
