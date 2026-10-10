//! Certified signatures over exact, bounded, versioned owner-period policy bytes.
use crate::{device_cert_message, Author};
use ed25519_dalek::{Signature, VerifyingKey};
use roda_types::owner_budget::{digest_id, OwnerPeriodPolicy, SignedOwnerPolicy};
use sha2::{Digest, Sha256};

fn message(policy: &OwnerPeriodPolicy) -> Option<Vec<u8>> {
    if !policy.valid_shape() {
        return None;
    }
    let bytes = serde_json::to_vec(policy).ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    let mut message = b"zoen-owner-period-policy-v1\0".to_vec();
    message.extend_from_slice(&bytes);
    Some(message)
}

fn strict(id: &str, message: &[u8], signature: &str) -> bool {
    if !digest_id(id) || signature.len() != 128 {
        return false;
    }
    let Some(bytes) = hex::decode(id)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
    else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&bytes) else {
        return false;
    };
    let Some(sig) = hex::decode(signature)
        .ok()
        .and_then(|v| <[u8; 64]>::try_from(v).ok())
    else {
        return false;
    };
    !key.is_weak()
        && key
            .verify_strict(message, &Signature::from_bytes(&sig))
            .is_ok()
}

pub fn sign(author: &Author, policy: OwnerPeriodPolicy) -> Option<SignedOwnerPolicy> {
    if policy.owner != author.identity || author.device.as_deref() != Some(&author.key.id()) {
        return None;
    }
    let signed = SignedOwnerPolicy {
        signature: author.key.sign(&message(&policy)?),
        policy,
        device: author.device.clone()?,
        cert: author.cert.clone()?,
    };
    verify(&signed).then_some(signed)
}

pub fn verify(signed: &SignedOwnerPolicy) -> bool {
    message(&signed.policy).is_some_and(|msg| {
        strict(
            &signed.policy.owner,
            &device_cert_message(&signed.device),
            &signed.cert,
        ) && strict(&signed.device, &msg, &signed.signature)
    })
}

pub fn digest(policy: &OwnerPeriodPolicy) -> Option<String> {
    Some(hex::encode(Sha256::digest(message(policy)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signer;
    #[test]
    fn every_policy_field_and_certified_signer_is_bound() {
        let author = Author::device(&Signer::generate(), Signer::generate());
        let policy = OwnerPeriodPolicy {
            owner: author.identity.clone(),
            year: 2026,
            month: 10,
            version: 1,
            previous_digest: None,
            currency: "USD".into(),
            scale: 6,
            limit_units: 1000,
            max_attempt_units: 100,
            expires_at_ms: 1_793_491_200_000,
            enabled: true,
            allowed_profiles: vec!["11".repeat(32)],
        };
        let signed = sign(&author, policy.clone()).unwrap();
        assert!(verify(&signed));
        assert_eq!(
            serde_json::from_slice::<SignedOwnerPolicy>(&serde_json::to_vec(&signed).unwrap())
                .unwrap(),
            signed
        );
        let mut changed = signed.clone();
        changed.policy.limit_units += 1;
        assert!(!verify(&changed));
        changed = signed.clone();
        changed.policy.allowed_profiles[0] = "22".repeat(32);
        assert!(!verify(&changed));
        changed = signed.clone();
        changed.policy.enabled = false;
        assert!(!verify(&changed));
        changed = signed.clone();
        changed.device = Signer::generate().id();
        assert!(!verify(&changed));
        changed = signed.clone();
        changed.cert = "00".repeat(64);
        assert!(!verify(&changed));
        assert!(sign(&Author::root(Signer::generate()), policy).is_none());
    }
}
