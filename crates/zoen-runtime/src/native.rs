//! Complete image custody. Deployment construction stays private and absent
//! until a managed key/credential backend is implemented. A fixture key is not
//! enrollment, relay currentness, restore continuity or dispatch authority.
mod runs;
mod storage;
pub(crate) use runs::{CleanupAcquisition, CleanupStep, ReplyStep};

use std::collections::BTreeMap;

use roda_ffi::runtime_core::{DeviceCore, NativeImage, UnlockedDevice};
use roda_types::Identity;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{custody::Custody, hash, id, NativeInspection, RuntimeAuthority, RuntimeError};

const IMAGE_BYTES: usize = 4 * 1024 * 1024;
const CHUNK_BYTES: usize = 64 * 1024;
const MAX_CHUNKS: usize = 64;
const MANIFEST_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Principal {
    owner: String,
    agent: String,
    device: String,
    certificate_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Context {
    format: u8,
    key_version: u32,
    principal: Principal,
    previous: Option<String>,
    generation: u64,
    capsule: String,
    image_bytes: u32,
    image_digest: String,
    chunks: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Root {
    context: Context,
    manifest_digest: String,
}

/// Inert exact target of one device maintenance admission. This is separate
/// from model admission and contains neither a Space grant nor a run permit.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ActivationScope {
    namespace: Vec<u8>,
    deployment: String,
    previous: Option<Root>,
    target: Root,
    fence: crate::execution::Fence,
    effects: String,
}

impl ActivationScope {
    pub(crate) fn matches_directory(
        &self,
        owner: &str,
        agent: &str,
        device: &str,
        certificate_digest: &str,
    ) -> bool {
        let principal = &self.target.context.principal;
        principal.owner == owner
            && principal.agent == agent
            && principal.device == device
            && principal.certificate_digest == certificate_digest
    }
}

#[derive(Serialize, Deserialize)]
struct Chunk {
    digest: String,
    bytes: u32,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    context: Context,
    chunks: Vec<Chunk>,
}

struct PreparedImage {
    root: Root,
    manifest: Vec<u8>,
    chunks: Vec<Vec<u8>>,
}

struct Credential {
    agent: Identity,
    owner: Identity,
    certificate: String,
    secret: Zeroizing<[u8; 32]>,
}

impl Credential {
    fn unlocked(&self) -> Result<UnlockedDevice, RuntimeError> {
        UnlockedDevice::verify(
            self.agent.clone(),
            self.owner.clone(),
            self.certificate.clone(),
            *self.secret,
        )
        .map_err(|_| RuntimeError::Denied)
    }

    fn principal(&self) -> Principal {
        Principal {
            owner: self.owner.id.clone(),
            agent: self.agent.id.clone(),
            device: roda_log::Signer::from_secret(&self.secret).id(),
            certificate_digest: hash(self.certificate.as_bytes()),
        }
    }
}

pub(super) struct NativeCustody {
    cipher: Custody,
    key_version: u32,
    // Only test provisioning currently constructs this private owner. No
    // public configuration accepts credential maps, images or signer secrets.
    credentials: BTreeMap<(String, String), Credential>,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

impl Context {
    fn validate(&self, principal: &Principal, key_version: u32) -> Result<(), RuntimeError> {
        if self.format != 1
            || self.key_version == 0
            || self.key_version != key_version
            || self.principal != *principal
            || ![
                &principal.owner,
                &principal.agent,
                &principal.device,
                &self.capsule,
            ]
            .into_iter()
            .all(|value| id(value))
            || !digest(&principal.certificate_digest)
            || !digest(&self.image_digest)
            || self.previous.as_deref().is_some_and(|d| !digest(d))
            || self.generation == 0
            || (self.generation == 1) != self.previous.is_none()
            || !(512..=IMAGE_BYTES).contains(&(self.image_bytes as usize))
            || self.chunks as usize != (self.image_bytes as usize).div_ceil(CHUNK_BYTES)
            || !(1..=MAX_CHUNKS).contains(&(self.chunks as usize))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }

    fn subject(&self, index: Option<usize>) -> Result<String, RuntimeError> {
        serde_json::to_string(&("native-image/1", self, index))
            .map_err(|_| RuntimeError::InvalidBinding)
    }
}

#[cfg(test)]
pub(crate) mod testing;

impl NativeCustody {
    fn credential(&self, agent: &str, device: &str) -> Result<&Credential, RuntimeError> {
        if !id(agent) || !id(device) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.credentials
            .get(&(agent.into(), device.into()))
            .ok_or(RuntimeError::CoreAuthorityUnavailable)
    }

    fn seal_image(
        &self,
        principal: &Principal,
        previous: Option<&Root>,
        image: &NativeImage,
    ) -> Result<PreparedImage, RuntimeError> {
        let bytes = image.as_bytes();
        if !(512..=IMAGE_BYTES).contains(&bytes.len()) {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let context = Context {
            format: 1,
            key_version: self.key_version,
            principal: principal.clone(),
            previous: previous.map(|r| r.manifest_digest.clone()),
            generation: previous
                .map(|r| r.context.generation)
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(RuntimeError::InvalidBinding)?,
            capsule: roda_types::new_id("capsule"),
            image_bytes: bytes.len() as u32,
            image_digest: hash(bytes),
            chunks: bytes.len().div_ceil(CHUNK_BYTES) as u32,
        };
        context.validate(principal, self.key_version)?;
        let mut chunks = Vec::new();
        let mut descriptors = Vec::new();
        for (index, plain) in bytes.chunks(CHUNK_BYTES).enumerate() {
            let (_, sealed) = self
                .cipher
                .seal_bytes(&context.subject(Some(index))?, plain)?;
            descriptors.push(Chunk {
                digest: hash(&sealed),
                bytes: sealed.len() as u32,
            });
            chunks.push(sealed);
        }
        let (_, manifest) = self.cipher.seal(
            &context.subject(None)?,
            &Manifest {
                context: context.clone(),
                chunks: descriptors,
            },
        )?;
        if manifest.len() >= MANIFEST_BYTES {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(PreparedImage {
            root: Root {
                context,
                manifest_digest: hash(&manifest),
            },
            manifest,
            chunks,
        })
    }

    fn open_image(
        &self,
        principal: &Principal,
        capsule: PreparedImage,
    ) -> Result<NativeImage, RuntimeError> {
        let context = &capsule.root.context;
        context.validate(principal, self.key_version)?;
        if !digest(&capsule.root.manifest_digest)
            || !(40..MANIFEST_BYTES).contains(&capsule.manifest.len())
            || hash(&capsule.manifest) != capsule.root.manifest_digest
            || capsule.chunks.len() != context.chunks as usize
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let manifest: Manifest =
            self.cipher
                .open(&context.subject(None)?, None, &capsule.manifest)?;
        if manifest.context != *context || manifest.chunks.len() != capsule.chunks.len() {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut image = Zeroizing::new(Vec::with_capacity(context.image_bytes as usize));
        for (index, (descriptor, sealed)) in manifest.chunks.iter().zip(&capsule.chunks).enumerate()
        {
            let plain_bytes = (context.image_bytes as usize - index * CHUNK_BYTES).min(CHUNK_BYTES);
            if sealed.len() != plain_bytes + 40
                || descriptor.bytes as usize != sealed.len()
                || !digest(&descriptor.digest)
                || hash(sealed) != descriptor.digest
            {
                return Err(RuntimeError::InvalidBinding);
            }
            let plain = self
                .cipher
                .open_bytes(&context.subject(Some(index))?, None, sealed)?;
            if plain.len() != plain_bytes {
                return Err(RuntimeError::InvalidBinding);
            }
            image.extend_from_slice(&plain);
        }
        if image.len() != context.image_bytes as usize || hash(&image) != context.image_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        NativeImage::decode_bounded(std::mem::take(&mut *image))
            .map_err(|_| RuntimeError::InvalidBinding)
    }

    pub(super) async fn inspect(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?;
        let capsule = runtime.execution.read_native(&principal, None).await?;
        let inspection = NativeInspection {
            generation: capsule.root.context.generation,
            image_bytes: capsule.root.context.image_bytes,
        };
        let image = self.open_image(&principal, capsule)?;
        DeviceCore::restore(image, credential.unlocked()?).map_err(|_| RuntimeError::Denied)?;
        guard.commit().await?;
        Ok(inspection)
    }

    pub(super) async fn repack(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
    ) -> Result<NativeInspection, RuntimeError> {
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        // Check real enrollment before acquiring a lease or staging any state.
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        runtime.execution.collect_native_stage(&principal).await?;
        runtime.execution.collect_native_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = self
            .repack_fenced(runtime, credential, &principal, &fence)
            .await;
        // Release cannot clear the monotonic token, nor release a new holder.
        // Any uncertain activation has already discarded its native workspace.
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(inspection) => {
                released?;
                Ok(inspection)
            }
        }
    }

    async fn repack_fenced(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        principal: &Principal,
        fence: &crate::execution::Fence,
    ) -> Result<NativeInspection, RuntimeError> {
        let capsule = runtime
            .execution
            .read_native(principal, Some(fence))
            .await?;
        let previous = capsule.root.clone();
        let next = {
            let image = self.open_image(principal, capsule)?;
            let core = DeviceCore::restore(image, credential.unlocked()?)
                .map_err(|_| RuntimeError::Denied)?;
            self.seal_image(
                principal,
                Some(&previous),
                &core.image().map_err(|_| RuntimeError::Denied)?,
            )?
        };
        let staged = runtime
            .execution
            .stage_native(principal, Some(previous), fence, next)
            .await?;
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 14 {
            runtime.native_cut_entered.notify_one();
            runtime.native_cut_resume.notified().await;
        }
        let admission = guard
            .authorize(staged.scope(&runtime.execution, fence))
            .await?;
        let root = runtime
            .execution
            .activate_native(staged, fence, admission)
            .await?;
        Ok(NativeInspection {
            generation: root.context.generation,
            image_bytes: root.context.image_bytes,
        })
    }
}
