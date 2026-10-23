//! Directory behind the unchanged native Client facade.

use super::*;
use crate::engine::Engine;

#[derive(Clone, PartialEq, Eq)]
struct DirectoryOwner {
    identity: String,
    device: String,
    relay: String,
    certificate: String,
}
impl DirectoryOwner {
    fn capture(engine: &Engine) -> Result<Self, CoreError> {
        let account = engine.account().ok_or_else(|| invalid("account"))?;
        engine.native_surface_owner(&account.identity, &account.device)?;
        Ok(Self {
            identity: account.identity.clone(),
            device: account.device.clone(),
            relay: account.relay_url.clone(),
            certificate: account.cert.clone(),
        })
    }
    fn check(&self, engine: &Engine) -> Result<(), CoreError> {
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
        engine.devices_arrived(list.clone());
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
    pub(super) fn signed_headers(
        &self,
        msg: impl Fn(i64) -> Vec<u8>,
    ) -> Result<(String, String, String), CoreError> {
        let e = self.lock();
        let author = e
            .net
            .author
            .as_ref()
            .ok_or_else(|| invalid(t("Entre na sua conta primeiro.", "Sign in first.")))?;
        let ts = crate::engine::now_ms();
        Ok((author.key.id(), ts.to_string(), author.key.sign(&msg(ts))))
    }

    pub(super) fn relay_base(&self) -> Result<String, CoreError> {
        let e = self.lock();
        let a = e.account().ok_or_else(|| invalid("account"))?;
        Ok(http_base(&a.relay_url))
    }

    pub(super) async fn unlink_device_impl(&self, device_id: String) -> Result<(), CoreError> {
        let (owner, reply) = self
            .device_directory_request(Op::Unlink {
                device: device_id.clone(),
            })
            .await?;
        match reply {
            Reply::Done => owner.unlinked(&mut self.lock(), &device_id),
            _ => Err(invalid("the relay answered something else")),
        }
    }

    pub(super) async fn devices_impl(&self) -> Result<Vec<DeviceDto>, CoreError> {
        let (owner, reply) = self.device_directory_request(Op::Devices).await?;
        let Reply::Devices(list) = reply else {
            return Err(invalid("the relay answered something else"));
        };
        owner.list(&mut self.lock(), list)
    }

    async fn device_directory_request(&self, op: Op) -> Result<(DirectoryOwner, Reply), CoreError> {
        // Same Net -> Engine lock order as start_sync. Keep the captured channel:
        // reconnect/account replacement cannot redirect an old intent to a new Net.
        let (owner, handle) = {
            let guard = self.net.lock().map_err(|_| CoreError::Storage {
                message: t(
                    "Não foi possível acessar os aparelhos.",
                    "Could not access devices.",
                ),
            })?;
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
            (owner, net.request_handle())
        };
        let reply = handle.send(op).await.map_err(invalid)?;
        Ok((owner, reply))
    }
}

#[cfg(test)]
mod tests;
