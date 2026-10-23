//! Directory behind the unchanged native Client facade.

use super::*;

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
        match self
            .request(Op::Unlink {
                device: device_id.clone(),
            })
            .await?
        {
            Reply::Done => self.lock().unlinked(&device_id),
            _ => Err(invalid("the relay answered something else")),
        }
    }

    pub(super) async fn devices_impl(&self) -> Result<Vec<DeviceDto>, CoreError> {
        let Reply::Devices(list) = self.request(Op::Devices).await? else {
            return Err(invalid("the relay answered something else"));
        };
        let mine = self
            .lock()
            .account()
            .map(|a| a.device.clone())
            .unwrap_or_default();
        self.lock().devices_arrived(list.clone());
        Ok(list
            .into_iter()
            .map(|d| DeviceDto {
                this_device: d.device == mine,
                device_id: d.device,
                revoked: d.revoked,
            })
            .collect())
    }
}
