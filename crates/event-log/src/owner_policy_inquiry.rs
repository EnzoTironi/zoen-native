//! Certified read-only inquiry. This domain cannot sign an owner spending policy.
use crate::{device_cert_message, owner_budget::strict, Author};
use sha2::{Digest, Sha256};
use types::owner_policy_context::{OwnerPolicyInquiry, SignedOwnerPolicyInquiry};
fn message(inquiry: &OwnerPolicyInquiry) -> Option<Vec<u8>> {
    if !inquiry.valid_shape() {
        return None;
    }
    let bytes = serde_json::to_vec(inquiry).ok()?;
    if bytes.len() > 2048 {
        return None;
    }
    let mut message = b"zoen-owner-policy-inquiry-v1\0".to_vec();
    message.extend_from_slice(&bytes);
    Some(message)
}
pub fn sign(author: &Author, inquiry: OwnerPolicyInquiry) -> Option<SignedOwnerPolicyInquiry> {
    if inquiry.owner != author.identity || author.device.as_deref() != Some(&author.key.id()) {
        return None;
    }
    let signed = SignedOwnerPolicyInquiry {
        signature: author.key.sign(&message(&inquiry)?),
        inquiry,
        device: author.device.clone()?,
        cert: author.cert.clone()?,
    };
    verify(&signed).then_some(signed)
}
pub fn verify(signed: &SignedOwnerPolicyInquiry) -> bool {
    message(&signed.inquiry).is_some_and(|message| {
        strict(
            &signed.inquiry.owner,
            &device_cert_message(&signed.device),
            &signed.cert,
        ) && strict(&signed.device, &message, &signed.signature)
    })
}
pub fn digest(inquiry: &OwnerPolicyInquiry) -> Option<String> {
    Some(hex::encode(Sha256::digest(message(inquiry)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inquiry_binds_review_target_profile_nonce_and_certified_human(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = crate::Signer::generate();
        let author = Author::device(&root, crate::Signer::generate());
        let inquiry = OwnerPolicyInquiry {
            id: "inquiry_nonce".into(),
            owner: root.id(),
            agent: crate::Signer::generate().id(),
            space: "space_1".into(),
            request: "rq_image_1".into(),
            content_hash: "11".repeat(32),
            price_digest: "22".repeat(32),
            at_ms: 100_000,
        };
        let signed = sign(&author, inquiry).ok_or("inquiry refused")?;
        assert!(verify(&signed));
        for field in [
            "id",
            "owner",
            "agent",
            "space",
            "request",
            "content_hash",
            "price_digest",
            "at_ms",
        ] {
            let mut value = serde_json::to_value(&signed)?;
            value["inquiry"][field] = if field == "at_ms" {
                100_001.into()
            } else {
                "different".into()
            };
            let tampered: SignedOwnerPolicyInquiry = serde_json::from_value(value)?;
            assert!(!verify(&tampered), "changed {field}");
        }
        assert!(signed.inquiry.fresh_at(100_000));
        assert!(!signed
            .inquiry
            .fresh_at(100_000 + types::owner_policy_context::CONTEXT_TTL_MS + 1));
        assert!(!signed.inquiry.fresh_at(69_999));
        assert!(sign(&Author::root(root), signed.inquiry).is_none());
        Ok(())
    }
}
