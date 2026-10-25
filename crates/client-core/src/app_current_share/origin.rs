//! Verify the existing factory's public App-ID preimage, not original plaintext
//! or historical bytes. No receipt key, seed hash or old file key is disclosed.
use super::*;
use types::{AppItemOrigin, ItemContent};
fn proof_valid(
    proof: &AppItemOrigin,
    app: &AppDoc,
    creation: &AppShareRef,
    space: &str,
    item: &str,
) -> bool {
    proof.author == creation.author
        && Some(proof.device.as_str()) == creation.device.as_deref()
        && proof.space == space
        && token(&proof.mutation)
        && miniapps::binary::Format::for_id(&app.app).is_some_and(|format| {
            crate::app_binary_documents::creation_item_id_for(
                format,
                &proof.author,
                &proof.device,
                space,
                &proof.mutation,
            )
            .is_ok_and(|id| id == item)
        })
}
pub(super) fn valid(app: &AppDoc, creation: &AppShareRef, space: &str, item: &str) -> bool {
    app.document
        .as_ref()
        .and_then(|file| file.app_origin.as_ref())
        .is_some_and(|proof| proof_valid(proof, app, creation, space, item))
}
pub(super) fn same_document(left: &AppDoc, right: &AppDoc) -> bool {
    let (mut left, mut right) = (left.clone(), right.clone());
    if let Some(file) = &mut left.document {
        file.app_origin = None;
    }
    if let Some(file) = &mut right.document {
        file.app_origin = None;
    }
    left == right
}
impl Engine {
    pub(super) fn app_share_origin(
        &self,
        item: &str,
        space: &str,
        creation: &AppShareRef,
    ) -> R<AppItemOrigin> {
        let state = self.item_state(item)?;
        let ItemContent::App(current) = &state.current().content else {
            return Err(refused());
        };
        if let Some(proof) = current
            .document
            .as_ref()
            .and_then(|file| file.app_origin.as_ref())
        {
            return proof_valid(proof, current, creation, space, item)
                .then(|| proof.clone())
                .ok_or_else(refused);
        }
        // A legacy peer can keep the public proof in an earlier known version or
        // signed sharing event. This reads metadata only, never that version's bytes.
        for version in state.versions.iter().rev() {
            if let ItemContent::App(app) = &version.content {
                if let Some(proof) = app
                    .document
                    .as_ref()
                    .and_then(|file| file.app_origin.as_ref())
                {
                    if proof_valid(proof, current, creation, space, item) {
                        return Ok(proof.clone());
                    }
                }
            }
        }
        if let Some(log) = self.logs.get(space) {
            for event in log.events().iter().rev() {
                if let EventBody::AppCurrentShared { share } = &event.body {
                    if share.item == item
                        && share.creation == *creation
                        && valid(&share.content, creation, space, item)
                    {
                        if let Some(proof) = share
                            .content
                            .document
                            .as_ref()
                            .and_then(|f| f.app_origin.clone())
                        {
                            return Ok(proof);
                        }
                    }
                }
            }
        }
        let format = miniapps::binary::Format::for_id(&current.app).ok_or_else(refused)?;
        if let Some(device) = &creation.device {
            if let Some(proof) =
                self.app_binary_creation_origin(format, space, item, &creation.author, device)?
            {
                return Ok(proof);
            }
        }
        Err(CoreError::Stale {
            reason: t(
                "Este App antigo ainda não pode ser compartilhado.",
                "This older App cannot be shared yet.",
            ),
        })
    }
}
