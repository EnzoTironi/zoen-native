//! Read-only transport binding, never restore continuity or an effect permit.
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const CAPABILITY: &str = "runtime-relay-binding-v1";
pub const MANAGED_CAPABILITY: &str = "managed-image-effects-v1";
pub const BLOB_HEADER: &str = "x-zoen-runtime-binding";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedCheckpoint {
    pub generation: u64,
    pub sequence: u64,
    pub digest: String,
}

impl ManagedCheckpoint {
    pub fn valid(&self) -> bool {
        self.generation > 0
            && self.generation <= i64::MAX as u64
            && self.sequence > 0
            && self.sequence <= i64::MAX as u64
            && hex32(&self.digest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub namespace: String,
    pub nonce: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed: Option<ManagedCheckpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub cell: String,
    pub tag: Vec<u8>,
}

pub fn valid_namespace(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl Request {
    pub fn valid(&self) -> bool {
        valid_namespace(&self.namespace)
            && self.nonce.len() == 32
            && self.managed.as_ref().is_none_or(ManagedCheckpoint::valid)
    }
}

/// Derived only by a trusted consumer from its private store pairing. Its MAC
/// state is deliberately not serializable or printable; the witness stays local.
pub struct Expectation {
    request: Request,
    cell: String,
    mac: Hmac<Sha256>,
}

impl Expectation {
    pub fn new(
        witness: &str,
        request: Request,
        cell: &str,
        relay: &str,
        identity: &str,
        device: &str,
    ) -> Option<Self> {
        if !request.valid()
            || !valid_namespace(cell)
            || relay.is_empty()
            || relay.len() > 256
            || identity.is_empty()
            || identity.len() > 128
            || !hex32(device)
            || !hex32(witness)
        {
            return None;
        }
        let mut key = [0u8; 32];
        hex::decode_to_slice(witness, &mut key).ok()?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).ok()?;
        let message = serde_json::to_vec(&(
            CAPABILITY,
            &request.namespace,
            &request.nonce,
            cell,
            relay,
            identity,
            device,
        ))
        .ok()?;
        mac.update(&message);
        // Preserve the existing unconfigured binding's exact MAC. Managed
        // effects add an explicit domain and checkpoint to the same pairing.
        if let Some(checkpoint) = &request.managed {
            mac.update(&serde_json::to_vec(&(MANAGED_CAPABILITY, checkpoint)).ok()?);
        }
        Some(Self {
            request,
            cell: cell.into(),
            mac,
        })
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn proof(&self) -> Proof {
        Proof {
            cell: self.cell.clone(),
            tag: self.mac.clone().finalize().into_bytes().to_vec(),
        }
    }

    pub fn verify(&self, proof: &Proof) -> bool {
        proof.cell == self.cell && self.mac.clone().verify_slice(&proof.tag).is_ok()
    }
}

fn hex32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The complete bound PUT signature prevents stripping the binding header and
/// replaying the upload as an ordinary, differently routed blob PUT.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobBinding {
    pub request: Request,
    pub proof: Proof,
    pub identity: String,
}

impl BlobBinding {
    pub fn message(&self, sha: &str, at_ms: i64, relay: &str) -> Option<Vec<u8>> {
        if !self.request.valid()
            || !valid_namespace(&self.proof.cell)
            || self.proof.tag.len() != 32
            || self.identity.is_empty()
            || self.identity.len() > 128
            || sha.len() != 64
        {
            return None;
        }
        serde_json::to_vec(&("zoen-runtime-blob-put/1", self, sha, at_ms, relay)).ok()
    }
}

#[cfg(test)]
mod tests;
