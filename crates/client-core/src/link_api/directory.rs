//! Directory behind the unchanged native Client facade.

use super::*;
use crate::{engine::Engine, net::RequestHandle};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DirectoryOwner {
    identity: String,
    device: String,
    relay: String,
    certificate: String,
}
impl DirectoryOwner {
    pub(super) fn capture(engine: &Engine) -> Result<Self, CoreError> {
        let account = engine.account().ok_or_else(|| invalid("account"))?;
        engine.native_surface_owner(&account.identity, &account.device)?;
        Ok(Self {
            identity: account.identity.clone(),
            device: account.device.clone(),
            relay: account.relay_url.clone(),
            certificate: account.cert.clone(),
        })
    }
    pub(super) fn check(&self, engine: &Engine) -> Result<(), CoreError> {
        if &Self::capture(engine)? != self {
            return Err(CoreError::Stale {
                reason: t(
                    "A conta deste aparelho mudou.",
                    "This device's account changed.",
                ),
            });
        }
        Ok(())
    }
    fn list(
        &self,
        engine: &mut Engine,
        list: Vec<protocol::DeviceRecord>,
    ) -> Result<Vec<DeviceDto>, CoreError> {
        self.check(engine)?;
        engine.devices_arrived(list.clone())?;
        Ok(list
            .into_iter()
            .map(|record| DeviceDto {
                this_device: record.device == self.device,
                device_id: record.device,
                revoked: record.revoked,
            })
            .collect())
    }
    fn unlinked(&self, engine: &mut Engine, device: &str) -> Result<(), CoreError> {
        self.check(engine)?;
        engine.unlinked(device)
    }
}

impl Client {
    pub(super) async fn unlink_device_impl(&self, device_id: String) -> Result<(), CoreError> {
        let (owner, channel, reply) = self
            .device_directory_request(Op::Unlink {
                device: device_id.clone(),
            })
            .await?;
        match reply {
            Reply::Done => self.with_device_directory_completion(&owner, &channel, |engine| {
                owner.unlinked(engine, &device_id)
            }),
            _ => Err(invalid("the relay answered something else")),
        }
    }

    pub(super) async fn devices_impl(&self) -> Result<Vec<DeviceDto>, CoreError> {
        let (owner, channel, reply) = self.device_directory_request(Op::Devices).await?;
        let Reply::Devices(list) = reply else {
            return Err(invalid("the relay answered something else"));
        };
        self.with_device_directory_completion(&owner, &channel, |engine| owner.list(engine, list))
    }

    pub(crate) fn with_device_directory_completion<T>(
        &self,
        owner: &DirectoryOwner,
        channel: &RequestHandle,
        apply: impl FnOnce(&mut Engine) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        // Hold Net until the checked Engine mutation finishes. A same-account
        // stop/start must not apply an old channel's completed reply to new state.
        let guard = self.net.lock().map_err(|_| directory_storage())?;
        let current = guard.as_ref().ok_or_else(directory_stale)?;
        if !channel.same_channel(&current.request_handle()) {
            return Err(directory_stale());
        }
        let mut engine = self.lock();
        owner.check(&engine)?;
        apply(&mut engine)
    }

    pub(crate) fn device_directory_owner(
        &self,
    ) -> Result<(DirectoryOwner, RequestHandle), CoreError> {
        let guard = self.net.lock().map_err(|_| directory_storage())?;
        let net = guard.as_ref().ok_or_else(|| invalid("offline"))?;
        let owner = DirectoryOwner::capture(&self.lock())?;
        if net
            .authenticated_relay_name(&owner.relay, &owner.identity, &owner.device)
            .is_none()
        {
            return Err(CoreError::Stale {
                reason: t(
                    "A conexão deste aparelho mudou.",
                    "This device's connection changed.",
                ),
            });
        }
        Ok((owner, net.request_handle()))
    }

    async fn device_directory_request(
        &self,
        op: Op,
    ) -> Result<(DirectoryOwner, RequestHandle, Reply), CoreError> {
        // Same Net -> Engine lock order as start_sync. Keep the captured channel:
        // reconnect/account replacement cannot redirect an old intent to a new Net.
        let (owner, handle) = self.device_directory_owner()?;
        let reply = handle.clone().send(op).await.map_err(invalid)?;
        Ok((owner, handle, reply))
    }
}

#[cfg(test)]
mod tests;

fn directory_storage() -> CoreError {
    CoreError::Storage {
        message: t(
            "Não foi possível acessar os aparelhos.",
            "Could not access devices.",
        ),
    }
}

fn directory_stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "A conexão deste aparelho mudou.",
            "This device's connection changed.",
        ),
    }
}

#[cfg(test)]
mod transport_tests;
