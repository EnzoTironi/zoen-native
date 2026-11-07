//! One retained owner intent, written before dispatch. Lost ACKs never mint a
//! second agent; connection completion is fenced by the existing Net lease.
use super::*;
use protocol::{
    managed_agents::{Record, Request, Service},
    AgentRequest,
};

const META: &str = "agent.enrollment.managed-intent.v1";
mod chat;
mod complete;
mod discard;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u8,
    owner: String,
    device: String,
    certificate_sha256: String,
    relay: String,
    request: Request,
}

impl Intent {
    fn load(engine: &crate::Engine, identity: &str, device: &str) -> R<Option<Self>> {
        let context = owner_context(engine, identity, device)?;
        let Some(value) = engine.store.meta(META)? else {
            return Ok(None);
        };
        if value.len() > 4096 {
            return Err(changed());
        }
        let intent: Self = serde_json::from_str(&value).map_err(|_| changed())?;
        let recipient = engine
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or_else(changed)?
            .public_hex();
        if intent.version != 1
            || !intent.request.valid()
            || intent.owner != identity
            || intent.device != device
            || intent.certificate_sha256 != context.certificate_sha256
            || intent.relay != context.relay
            || intent.request.recipient_public_key != recipient
        {
            return Err(changed());
        }
        Ok(Some(intent))
    }

    fn prepare(
        engine: &crate::Engine,
        service: &Service,
        name: &str,
        handle: &str,
        identity: &str,
        device: &str,
    ) -> R<Request> {
        let context = owner_context(engine, identity, device)?;
        if let Some(held) = Self::load(engine, identity, device)? {
            if held.request.service.as_ref() != Some(service)
                || held.request.name != name
                || held.request.handle != handle
            {
                return Err(changed());
            }
            return Ok(held.request);
        }
        let request = Request {
            operation: hex::encode(crate::link::random::<32>()?),
            service: Some(service.clone()),
            name: name.into(),
            handle: handle.into(),
            recipient_public_key: engine
                .net
                .profiles
                .agreement
                .as_ref()
                .ok_or_else(changed)?
                .public_hex(),
        };
        if !request.valid() {
            return Err(malformed());
        }
        let intent = Self {
            version: 1,
            owner: identity.into(),
            device: device.into(),
            certificate_sha256: context.certificate_sha256,
            relay: context.relay,
            request: request.clone(),
        };
        let value = serde_json::to_string(&intent).map_err(|_| malformed())?;
        engine.store.set_meta(META, &value)?;
        if engine.store.meta(META)?.as_deref() != Some(value.as_str()) {
            return Err(changed());
        }
        Ok(request)
    }
}

impl Client {
    /// Local recovery does not require a live service or create another request.
    pub fn pending_managed_enrollment(&self, identity: &str, device: &str) -> R<Option<Request>> {
        Ok(Intent::load(&self.lock(), identity, device)?.map(|intent| intent.request))
    }

    pub async fn managed_agent_service(&self, identity: &str, device: &str) -> R<Option<Service>> {
        let (owner, channel) = self.device_directory_owner()?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            owner_context(engine, identity, device)?;
            Ok(())
        })?;
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::ManagedService))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            owner_context(engine, identity, device)?;
            let Reply::ManagedService(availability) = response else {
                return Err(changed());
            };
            if availability
                .service
                .as_ref()
                .is_some_and(|service| !service.valid())
            {
                return Err(changed());
            }
            Ok(availability.service)
        })
    }

    /// Exact replay is deliberate. This never replaces the retained operation or
    /// chooses a different service following an ambiguous response.
    pub async fn prepare_managed_enrollment(
        &self,
        service: &Service,
        name: &str,
        handle: &str,
        identity: &str,
        device: &str,
    ) -> R<Record> {
        let (owner, channel) = self.device_directory_owner()?;
        let request = self.with_device_directory_completion(&owner, &channel, |engine| {
            Intent::prepare(engine, service, name, handle, identity, device)
        })?;
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::PrepareManaged(request.clone())))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            managed_record(engine, &request, response, identity, device)
        })
    }

    pub async fn managed_enrollment_status(&self, identity: &str, device: &str) -> R<Record> {
        let (owner, channel) = self.device_directory_owner()?;
        let request = self.with_device_directory_completion(&owner, &channel, |engine| {
            Ok(Intent::load(engine, identity, device)?
                .ok_or_else(changed)?
                .request)
        })?;
        let service = request.service.as_ref().ok_or_else(changed)?;
        let response = channel
            .clone()
            .send(Op::Agent(AgentRequest::ManagedStatus {
                service: service.id.clone(),
                operation: request.operation.clone(),
            }))
            .await
            .map_err(invalid)?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            managed_record(engine, &request, response, identity, device)
        })
    }
}

fn managed_record(
    engine: &crate::Engine,
    request: &Request,
    response: Reply,
    identity: &str,
    device: &str,
) -> R<Record> {
    let intent = Intent::load(engine, identity, device)?.ok_or_else(changed)?;
    let Reply::ManagedAgent(record) = response else {
        return Err(changed());
    };
    if intent.request != *request
        || !record.valid()
        || record.owner != identity
        || record.request.as_ref() != Some(request)
    {
        return Err(changed());
    }
    Ok(record)
}

#[cfg(test)]
mod tests;
