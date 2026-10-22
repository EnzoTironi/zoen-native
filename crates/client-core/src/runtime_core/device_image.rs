//! Device image operations on the original DeviceCore state.

use super::*;

impl DeviceCore {
    pub fn bootstrap(device: UnlockedDevice, relay_url: &str) -> Result<Self> {
        if relay_url.is_empty() || relay_url.len() > 2048 {
            return Err(NativeError::Bounds);
        }
        let mut engine = Engine::open(":memory:").map_err(|_| NativeError::Storage)?;
        limit_pages(&engine.store)?;
        engine
            .store
            .put_identity(&device.agent, None)
            .and_then(|_| engine.store.put_identity(&device.owner, None))
            .and_then(|_| engine.store.set_meta("me", &device.agent.id))
            .map_err(|_| NativeError::Storage)?;
        let account = AccountMeta {
            identity: device.agent.id.clone(),
            device: device.leaf.device.clone(),
            cert: device.leaf.cert.clone(),
            relay_url: relay_url.into(),
            registered: false,
        };
        engine
            .save_linked_account(account)
            .map_err(|_| NativeError::Storage)?;
        engine.reload().map_err(|_| NativeError::Storage)?;
        Self::install(engine, device)
    }

    pub fn restore(image: NativeImage, device: UnlockedDevice) -> Result<Self> {
        let store = Store::from_memory_image(image.as_bytes()).map_err(|_| NativeError::Image)?;
        validate_image(&store)?;
        let engine = Engine::from_store(store, ":memory:").map_err(|_| NativeError::Image)?;
        Self::install(engine, device)
    }

    pub(super) fn install(mut engine: Engine, device: UnlockedDevice) -> Result<Self> {
        let account = engine.net.account.as_ref().ok_or(NativeError::Credential)?;
        if account.identity != device.agent.id
            || account.device != device.leaf.device
            || account.cert != device.leaf.cert
            || engine.me.as_deref() != Some(device.agent.id.as_str())
            || engine.identities.get(&device.agent.id) != Some(&device.agent)
            || engine.identities.get(&device.owner.id) != Some(&device.owner)
            || engine
                .state
                .spaces
                .values()
                .any(|space| space.integrity_error.is_some())
        {
            return Err(NativeError::Credential);
        }
        engine.net.author = Some(Author::certified(
            device.agent.id.clone(),
            Signer::from_secret(&device.secret),
            device.leaf.cert,
        ));
        engine.device().map_err(|_| NativeError::Credential)?;
        Ok(Self {
            engine,
            agent: device.agent.id,
            owner: device.owner.id,
            usable: true,
        })
    }

    pub fn image(&self) -> Result<NativeImage> {
        if !self.usable {
            return Err(NativeError::History);
        }
        let bytes = self
            .engine
            .store
            .memory_image()
            .map_err(|_| NativeError::Image)?;
        Ok(NativeImage(Zeroizing::new(bytes)))
    }

    pub fn key_packages(&mut self, count: usize) -> Result<(Vec<Vec<u8>>, NativeImage)> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if !(1..=32).contains(&count) {
            return Err(NativeError::Bounds);
        }
        let result = (|| {
            let packages = self
                .engine
                .device()
                .map_err(|_| NativeError::Credential)?
                .key_packages(count, false)
                .map_err(|_| NativeError::Storage)?;
            Ok((packages, self.image()?))
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }

    pub fn ingest(&mut self, batch: Vec<Sequenced>) -> Result<NativeImage> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if batch.len() > MAX_BATCH
            || batch
                .iter()
                .any(|entry| entry.env.content().len() > MAX_EVENT_BYTES)
        {
            return Err(NativeError::Bounds);
        }
        for entry in batch {
            if let Err(error) = self.guard_pending_echo(&entry) {
                self.usable = false;
                return Err(error);
            }
            match self.engine.ingest(entry) {
                Ingest::Applied | Ingest::Confirmed | Ingest::Duplicate => {}
                Ingest::Gap { .. } | Ingest::Invalid(_) => {
                    self.usable = false;
                    return Err(NativeError::History);
                }
            }
        }
        let image = self.image();
        if image.is_err() {
            self.usable = false;
        }
        image
    }
}
