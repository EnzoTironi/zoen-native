//! Agent ownership requires a signature from an owner's certified device.

use crate::{device_cert_message, Author};
use ed25519_dalek::{Signature, VerifyingKey};
use types::{AgentOwnerProof, Identity, IdentityKind};

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
#[must_use]
pub fn message(owner: &str, agent: &str, device: &str) -> Vec<u8> {
    format!("zoen-agent-owner-v1\0{owner}\0{agent}\0{device}").into_bytes()
}

#[must_use]
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
#[must_use]
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

/// The Agent signs the digest of a canonical enrollment offer in its own domain.
#[must_use]
pub fn enrollment_offer_message(agent: &str, offer_sha256: &str) -> Option<Vec<u8>> {
    key(agent)?;
    if offer_sha256.len() != 64
        || !offer_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(format!("zoen-agent-enrollment-offer-v1\0{agent}\0{offer_sha256}").into_bytes())
}

#[must_use]
pub fn verify_enrollment_offer(agent: &str, offer_sha256: &str, signature: &str) -> bool {
    let (Some(agent_key), Some(message)) =
        (key(agent), enrollment_offer_message(agent, offer_sha256))
    else {
        return false;
    };
    verified(&agent_key, &message, signature)
}

fn enrollment_owner_message(
    owner: &str,
    agent: &str,
    offer_sha256: &str,
    proof: &AgentOwnerProof,
) -> Option<Vec<u8>> {
    key(owner)?;
    key(&proof.device)?;
    enrollment_offer_message(agent, offer_sha256)?;
    let proof_bytes = serde_json::to_vec(&(
        proof.device.as_str(),
        proof.cert.as_str(),
        proof.signature.as_str(),
    ))
    .ok()?;
    let proof_sha256 = crate::sha256_hex(&proof_bytes);
    Some(
        format!(
            "zoen-agent-enrollment-owner-v1\0{owner}\0{agent}\0{}\0{offer_sha256}\0{proof_sha256}",
            proof.device
        )
        .into_bytes(),
    )
}

/// Bind this offer to an ordinary ownership proof using the same certified device.
#[must_use]
pub fn authorize_enrollment(
    owner: &Author,
    agent: &str,
    offer_sha256: &str,
) -> Option<(AgentOwnerProof, String)> {
    let proof = authorize(owner, agent)?;
    let message = enrollment_owner_message(&owner.identity, agent, offer_sha256, &proof)?;
    let signature = owner.key.sign(&message);
    Some((proof, signature))
}

#[must_use]
pub fn verify_enrollment(
    owner: &str,
    agent: &str,
    offer_sha256: &str,
    proof: &AgentOwnerProof,
    signature: &str,
) -> bool {
    if !verify(owner, agent, proof) {
        return false;
    }
    let (Some(device_key), Some(message)) = (
        key(&proof.device),
        enrollment_owner_message(owner, agent, offer_sha256, proof),
    ) else {
        return false;
    };
    verified(&device_key, &message, signature)
}

#[must_use]
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

    #[test]
    fn enrollment_signatures_follow_the_explicit_wire_domains() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate();
        let offer_sha256 = "ab".repeat(32);
        let agent_id = agent.id();
        let offer_message = enrollment_offer_message(&agent_id, &offer_sha256).unwrap();
        assert_eq!(
            offer_message,
            format!("zoen-agent-enrollment-offer-v1\0{agent_id}\0{offer_sha256}").into_bytes()
        );
        let offer_signature = agent.sign(&offer_message);
        assert!(verify_enrollment_offer(
            &agent_id,
            &offer_sha256,
            &offer_signature
        ));

        let (proof, signature) = authorize_enrollment(&owner, &agent_id, &offer_sha256).unwrap();
        assert!(verify(&root.id(), &agent_id, &proof));
        // Hex strings need no JSON escaping; this fixes the tuple order and encoding.
        let proof_json = format!(
            "[\"{}\",\"{}\",\"{}\"]",
            proof.device, proof.cert, proof.signature
        );
        let proof_sha256 = crate::sha256_hex(proof_json.as_bytes());
        let owner_message = format!(
            "zoen-agent-enrollment-owner-v1\0{}\0{agent_id}\0{}\0{offer_sha256}\0{proof_sha256}",
            root.id(),
            proof.device
        );
        assert_eq!(signature, owner.key.sign(owner_message.as_bytes()));
        assert!(verify_enrollment(
            &root.id(),
            &agent_id,
            &offer_sha256,
            &proof,
            &signature
        ));
    }

    #[test]
    fn enrollment_rejects_noncanonical_digests_and_agent_keys() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate();
        let agent_id = agent.id();
        let digest = "ab".repeat(32);
        let offer_signature = agent.sign(&enrollment_offer_message(&agent_id, &digest).unwrap());
        let (proof, signature) = authorize_enrollment(&owner, &agent_id, &digest).unwrap();
        for invalid in [
            String::new(),
            "ab".repeat(31),
            "ab".repeat(33),
            "AB".repeat(32),
            "g0".repeat(32),
            format!("{}\0", "a".repeat(63)),
            format!("{}\n", "a".repeat(63)),
            "é".repeat(32),
        ] {
            assert!(enrollment_offer_message(&agent_id, &invalid).is_none());
            assert!(!verify_enrollment_offer(
                &agent_id,
                &invalid,
                &offer_signature
            ));
            assert!(authorize_enrollment(&owner, &agent_id, &invalid).is_none());
            assert!(!verify_enrollment(
                &root.id(),
                &agent_id,
                &invalid,
                &proof,
                &signature
            ));
        }
        for invalid in [
            String::new(),
            "not-a-key".into(),
            agent_id.to_uppercase(),
            format!("{}\0", &agent_id[..63]),
            "00".repeat(32),
            format!("01{}", "00".repeat(31)),
        ] {
            assert!(enrollment_offer_message(&invalid, &digest).is_none());
            assert!(!verify_enrollment_offer(
                &invalid,
                &digest,
                &offer_signature
            ));
            assert!(authorize_enrollment(&owner, &invalid, &digest).is_none());
            assert!(!verify_enrollment(
                &root.id(),
                &invalid,
                &digest,
                &proof,
                &signature
            ));
        }
    }

    #[test]
    fn enrollment_offer_signature_is_bound_to_agent_and_digest() {
        let agent = Signer::generate();
        let other = Signer::generate();
        let digest = "ab".repeat(32);
        let message = enrollment_offer_message(&agent.id(), &digest).unwrap();
        let signature = agent.sign(&message);
        assert!(!verify_enrollment_offer(&other.id(), &digest, &signature));
        assert!(!verify_enrollment_offer(
            &agent.id(),
            &"cd".repeat(32),
            &signature
        ));
        assert!(!verify_enrollment_offer(
            &agent.id(),
            &digest,
            &other.sign(&message)
        ));
        assert!(!verify_enrollment_offer(
            &agent.id(),
            &digest,
            &agent.sign(digest.as_bytes())
        ));
    }

    #[test]
    fn enrollment_owner_signature_is_bound_to_every_proof_field() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate().id();
        let digest = "ab".repeat(32);
        let (proof, signature) = authorize_enrollment(&owner, &agent, &digest).unwrap();
        assert!(!verify_enrollment(
            &Signer::generate().id(),
            &agent,
            &digest,
            &proof,
            &signature
        ));
        assert!(!verify_enrollment(
            &root.id(),
            &Signer::generate().id(),
            &digest,
            &proof,
            &signature
        ));
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &"cd".repeat(32),
            &proof,
            &signature
        ));
        let mut changed = proof.clone();
        changed.device = Signer::generate().id();
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &changed,
            &signature
        ));
        changed = proof.clone();
        changed.cert = Signer::generate().sign(&device_cert_message(&proof.device));
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &changed,
            &signature
        ));
        changed = proof.clone();
        changed.signature = owner.key.sign(b"different ownership proof");
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &changed,
            &signature
        ));

        // Existing ownership verification accepts hex case; enrollment binds its exact bytes.
        for change_cert in [false, true] {
            changed = proof.clone();
            if change_cert {
                changed.cert.make_ascii_uppercase();
            } else {
                changed.signature.make_ascii_uppercase();
            }
            assert!(verify(&root.id(), &agent, &changed));
            assert!(!verify_enrollment(
                &root.id(),
                &agent,
                &digest,
                &changed,
                &signature
            ));
        }
    }

    #[test]
    fn enrollment_requires_the_same_certified_device_for_both_signatures() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let other_device = Author::device(&root, Signer::generate());
        let agent = Signer::generate().id();
        let digest = "ab".repeat(32);
        let (proof, signature) = authorize_enrollment(&owner, &agent, &digest).unwrap();
        let message = enrollment_owner_message(&root.id(), &agent, &digest, &proof).unwrap();
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &proof,
            &other_device.key.sign(&message)
        ));
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &proof,
            &root.sign(&message)
        ));
        let (other_proof, other_signature) =
            authorize_enrollment(&other_device, &agent, &digest).unwrap();
        assert!(verify(&root.id(), &agent, &other_proof));
        assert!(verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &other_proof,
            &other_signature
        ));
        assert!(!verify_enrollment(
            &root.id(),
            &agent,
            &digest,
            &other_proof,
            &signature
        ));
    }

    #[test]
    fn generic_ownership_and_offer_signatures_cannot_authorize_enrollment() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate();
        let digest = "ab".repeat(32);
        let (proof, signature) = authorize_enrollment(&owner, &agent.id(), &digest).unwrap();
        let offer_message = enrollment_offer_message(&agent.id(), &digest).unwrap();
        for substitution in [
            proof.signature.clone(),
            proof.cert.clone(),
            agent.sign(&offer_message),
            owner.key.sign(&offer_message),
        ] {
            assert!(!verify_enrollment(
                &root.id(),
                &agent.id(),
                &digest,
                &proof,
                &substitution
            ));
        }
        assert!(!verify_enrollment_offer(&agent.id(), &digest, &signature));
        assert!(!verify(
            &root.id(),
            &agent.id(),
            &AgentOwnerProof { signature, ..proof }
        ));
    }

    #[test]
    fn enrollment_rejects_uncertified_authors_and_weak_owner_or_device_keys() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate().id();
        let digest = "ab".repeat(32);
        assert!(authorize_enrollment(&Author::root(root.clone()), &agent, &digest).is_none());
        let fake = Author::certified(root.id(), Signer::generate(), "00".repeat(64));
        assert!(authorize_enrollment(&fake, &agent, &digest).is_none());
        let mut mismatched = owner.clone();
        mismatched.device = Some(Signer::generate().id());
        assert!(authorize_enrollment(&mismatched, &agent, &digest).is_none());
        let mut uncertified = owner.clone();
        uncertified.cert = None;
        assert!(authorize_enrollment(&uncertified, &agent, &digest).is_none());
        assert!(authorize_enrollment(&owner, &root.id(), &digest).is_none());

        let (proof, signature) = authorize_enrollment(&owner, &agent, &digest).unwrap();
        for invalid in [
            root.id().to_uppercase(),
            "00".repeat(32),
            format!("01{}", "00".repeat(31)),
        ] {
            assert!(!verify_enrollment(
                &invalid, &agent, &digest, &proof, &signature
            ));
            let mut changed = proof.clone();
            changed.device = invalid;
            assert!(!verify_enrollment(
                &root.id(),
                &agent,
                &digest,
                &changed,
                &signature
            ));
        }
    }

    #[test]
    fn enrollment_rejects_malformed_and_invalid_signatures() {
        let root = Signer::generate();
        let owner = Author::device(&root, Signer::generate());
        let agent = Signer::generate();
        let digest = "ab".repeat(32);
        let (proof, _) = authorize_enrollment(&owner, &agent.id(), &digest).unwrap();
        for invalid in [
            String::new(),
            "00".repeat(63),
            "00".repeat(65),
            "gg".repeat(64),
            "00".repeat(64),
            "ff".repeat(64),
        ] {
            assert!(!verify_enrollment_offer(&agent.id(), &digest, &invalid));
            assert!(!verify_enrollment(
                &root.id(),
                &agent.id(),
                &digest,
                &proof,
                &invalid
            ));
        }
    }
}
