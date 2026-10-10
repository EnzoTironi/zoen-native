//! Agent ownership requires a signature from an owner's certified device.

use crate::{device_cert_message, Author};
use ed25519_dalek::{Signature, VerifyingKey};
use roda_types::{AgentOwnerProof, Identity, IdentityKind};

fn key(id: &str) -> Option<VerifyingKey> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let bytes: [u8; 32] = hex::decode(id).ok()?.try_into().ok()?;
    let key = VerifyingKey::from_bytes(&bytes).ok()?;
    (!key.is_weak()).then_some(key)
}

fn verified(key: &VerifyingKey, message: &[u8], signature: &str) -> bool {
    if signature.len() != 128 {
        return false;
    }
    let Some(bytes) = hex::decode(signature)
        .ok()
        .and_then(|s| <[u8; 64]>::try_from(s).ok())
    else {
        return false;
    };
    key.verify_strict(message, &Signature::from_bytes(&bytes))
        .is_ok()
}

/// All three ids are canonical 64-character hex public keys, separated by a NUL.
pub fn message(owner: &str, agent: &str, device: &str) -> Vec<u8> {
    format!("zoen-agent-owner-v1\0{owner}\0{agent}\0{device}").into_bytes()
}

pub fn verify(owner: &str, agent: &str, proof: &AgentOwnerProof) -> bool {
    if owner == agent || key(agent).is_none() {
        return false;
    }
    let (Some(owner_key), Some(device_key)) = (key(owner), key(&proof.device)) else {
        return false;
    };
    verified(&owner_key, &device_cert_message(&proof.device), &proof.cert)
        && verified(
            &device_key,
            &message(owner, agent, &proof.device),
            &proof.signature,
        )
}

/// Signing requires a certified device. Relay admission also checks its current
/// enrollment and revocation state; possession of this proof cannot revive a device.
pub fn authorize(owner: &Author, agent: &str) -> Option<AgentOwnerProof> {
    let device = owner.device.as_ref()?;
    if device != &owner.key.id() {
        return None;
    }
    let proof = AgentOwnerProof {
        device: device.clone(),
        cert: owner.cert.clone()?,
        signature: owner.key.sign(&message(&owner.identity, agent, device)),
    };
    verify(&owner.identity, agent, &proof).then_some(proof)
}

pub fn profile_authorized(profile: &Identity) -> bool {
    match profile.kind {
        IdentityKind::Person => profile.owner.is_none() && profile.owner_proof.is_none(),
        IdentityKind::Agent => match (&profile.owner, &profile.owner_proof) {
            (Some(owner), Some(proof)) => verify(owner, &profile.id, proof),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signer;

    #[test]
    fn ownership_is_bound_to_owner_agent_and_device() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate().id();
        let proof = authorize(&owner, &agent).unwrap();
        assert!(verify(&root.id(), &agent, &proof));
        assert!(!verify(&Signer::generate().id(), &agent, &proof));
        assert!(!verify(&root.id(), &Signer::generate().id(), &proof));
        let mut changed = proof.clone();
        changed.device = Signer::generate().id();
        assert!(!verify(&root.id(), &agent, &changed));
        changed = proof.clone();
        changed.cert = Signer::generate().sign(&device_cert_message(&proof.device));
        assert!(!verify(&root.id(), &agent, &changed));
        changed = proof;
        changed.signature = owner.key.sign(&device_cert_message(&agent));
        assert!(!verify(&root.id(), &agent, &changed));
    }

    #[test]
    fn root_only_and_falsely_certified_authors_cannot_issue_proofs() {
        let root = Signer::generate();
        let agent = Signer::generate().id();
        assert!(authorize(&Author::root(root.clone()), &agent).is_none());
        let fake = Author::certified(root.id(), Signer::generate(), "00".repeat(64));
        assert!(authorize(&fake, &agent).is_none());
        let owner = Author::device(&root, Signer::generate());
        assert!(authorize(&owner, &root.id()).is_none());
        assert!(authorize(&owner, "not-a-key").is_none());
        assert!(authorize(&owner, &"00".repeat(32)).is_none());
    }
}
