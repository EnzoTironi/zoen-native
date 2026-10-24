use super::*;
use protocol::complete_reads::{self as reads, Assembly, Kind, Request};
impl<S: Signer> Client<S> {
    /// The browser host passes only the actual authenticated connection's negotiated
    /// capability. Bootstrap clears staging on reconnect, never durable device sets.
    pub fn read_capability(&mut self, enabled: bool) {
        self.membership_net.read_capability = enabled;
        self.membership_net.devices_assembly = None;
    }
    pub(super) fn device_directory_cancel(&mut self, outgoing: &mut Vec<Vec<u8>>) {
        if let Some(request) = self
            .membership_net
            .devices_assembly
            .take()
            .and_then(|a| a.cancel_request())
        {
            outgoing.push(
                ClientFrame::Req {
                    id: self.membership_net.devices_request,
                    op: Op::CompleteRead(request),
                }
                .encode(),
            );
        }
    }
    pub(super) fn device_directory_begin(&mut self) -> Result<Op, String> {
        self.membership_net.devices_generation = self
            .membership_net
            .devices_generation
            .checked_add(1)
            .ok_or(reads::PROFILE_EXCEEDED)?;
        self.membership_net.devices_request =
            DEVICES_REQUEST_BASE + u64::from(self.membership_net.devices_generation);
        if self.membership_net.read_capability {
            self.membership_net.devices_assembly = Some(Assembly::new(
                Kind::Devices,
                self.account.identity.clone(),
                self.account.device.clone(),
            ));
            Ok(Op::CompleteRead(Request::begin(Kind::Devices)))
        } else {
            Ok(Op::Devices)
        }
    }
    pub(super) fn device_directory_reply(
        &mut self,
        result: &Result<Reply, String>,
        outgoing: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if self.membership_net.devices_requested
            && self.membership_net.devices_due <= self.view_now_ms
        {
            self.device_directory_cancel(outgoing);
        }
        let full = if let Some(mut assembly) = self.membership_net.devices_assembly.take() {
            let cancel = assembly.cancel_request();
            let full = match result {
                Ok(Reply::CompletePage(page)) => match assembly.accept(page.clone()) {
                    Ok(Some(request)) => {
                        outgoing.push(
                            ClientFrame::Req {
                                id: self.membership_net.devices_request,
                                op: Op::CompleteRead(request),
                            }
                            .encode(),
                        );
                        self.membership_net.devices_assembly = Some(assembly);
                        return Ok(());
                    }
                    Ok(None) => assembly.finish().map_err(str::to_owned),
                    Err(e) => Err(e.into()),
                },
                Ok(_) => Err(reads::INVALID.into()),
                Err(e) => Err(e.clone()),
            };
            if full.is_err() {
                if let Some(request) = cancel {
                    outgoing.push(
                        ClientFrame::Req {
                            id: self.membership_net.devices_request,
                            op: Op::CompleteRead(request),
                        }
                        .encode(),
                    );
                }
            }
            full
        } else if self.membership_net.read_capability {
            // Late/duplicate/foreign responses cannot create a new assembly or
            // overwrite the last complete persisted revoked-device projection.
            Err(reads::INVALID.into())
        } else {
            result.clone()
        };
        self.membership_net.devices_requested = false;
        self.membership_net.devices_due =
            self.view_now_ms
                .saturating_add(if full.is_ok() { 60_000 } else { 5000 });
        if let Ok(Reply::Devices(devices)) = full {
            let mut seen = BTreeSet::new();
            if devices.len() > reads::DEVICE_ROWS
                || devices.iter().any(|d| {
                    d.device.len() != 64
                        || !d.device.bytes().all(|b| b.is_ascii_hexdigit())
                        || !seen.insert(d.device.clone())
                })
            {
                self.membership_net.devices_due = self.view_now_ms.saturating_add(5000);
                return Ok(());
            }
            self.saved.membership.revoked = devices
                .into_iter()
                .filter(|d| d.revoked)
                .map(|d| d.device)
                .collect();
        }
        // Existing Client::update + host persistBeforeOutgoing owns one complete
        // SavedClient write. A page itself never mutates saved membership.
        Ok(())
    }
}
