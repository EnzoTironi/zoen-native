//! Public MLS context for external recovery joins (RFC 9420 §12.4.3).

use std::collections::BTreeSet;

use openmls::{
    ciphersuite::{signable::Verifiable, signature::OpenMlsSignaturePublicKey},
    messages::group_info::VerifiableGroupInfo,
    prelude::{tls_codec::Deserialize as _, *},
};
use openmls_rust_crypto::OpenMlsRustCrypto;

use crate::{leaf_of, mls, MlsError, SUITE};

pub const MAX_GROUP_INFO_BYTES: usize = roda_log::recovery::MAX_CONTEXT_BYTES;

pub struct GroupInfoSummary {
    pub epoch: u64,
    pub identities: BTreeSet<String>,
}

/// Verify the signature, complete public tree, device certificates and group.
/// A publication must also be signed by its outer envelope's device.
pub fn verify_group_info(
    space: &str,
    data: &[u8],
    publishing_device: Option<&str>,
) -> Result<GroupInfoSummary, MlsError> {
    if data.is_empty() || data.len() > MAX_GROUP_INFO_BYTES {
        return Err(MlsError::Decode("GroupInfo size is out of bounds".into()));
    }
    let info = VerifiableGroupInfo::tls_deserialize_exact(data)
        .map_err(|e| MlsError::Decode(e.to_string()))?;
    if info.ciphersuite() != SUITE || info.group_id().as_slice() != space.as_bytes() {
        return Err(MlsError::Mls("GroupInfo for another group or suite".into()));
    }
    let tree = info
        .extensions()
        .ratchet_tree()
        .ok_or_else(|| MlsError::Mls("GroupInfo lacks its public tree".into()))?
        .ratchet_tree()
        .clone();
    let provider = OpenMlsRustCrypto::default();
    if let Some(device) = publishing_device {
        let key = OpenMlsSignaturePublicKey::new(
            hex::decode(device).map_err(mls)?.into(),
            SUITE.signature_algorithm(),
        )
        .map_err(mls)?;
        info.verify_no_out(provider.crypto(), &key).map_err(mls)?;
    }
    let (group, _) = PublicGroup::from_external(
        provider.crypto(),
        provider.storage(),
        tree,
        info,
        ProposalStore::new(),
    )
    .map_err(mls)?;
    let identities = group
        .members()
        .map(|member| leaf_of(&member.credential, &member.signature_key).map(|leaf| leaf.identity))
        .collect::<Result<_, MlsError>>()?;
    Ok(GroupInfoSummary {
        epoch: group.group_context().epoch().as_u64(),
        identities,
    })
}
