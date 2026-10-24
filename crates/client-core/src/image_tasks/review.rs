use super::*;
use crate::ImageToolReview;

/// Projection of the same canonical signed arguments used at execution admission.
pub(crate) fn project(proposal: &ToolProposal) -> Option<ImageToolReview> {
    let (_, arguments) = parsed(proposal).ok()?;
    let input: ImageTaskInput = serde_json::from_value(arguments?).ok()?;
    Some(ImageToolReview {
        prompt: input.prompt,
        size: input.definition.size,
        quality: input.definition.quality,
        background: input.definition.background,
        definition_version: input.definition.version,
        price_digest: input.definition.price_digest,
        max_image_bytes: input.definition.max_image_bytes,
        source_id: proposal.source.clone(),
        source_sha256: proposal.source_sha256.clone(),
    })
}

impl Engine {
    pub(crate) fn image_generation_review(
        &self,
        proposal: &ToolProposal,
    ) -> Option<ImageToolReview> {
        project(proposal)
    }

    pub(crate) fn stage_scoped_image_generation(
        &mut self,
        scope: &ImageTaskScope,
        source: &str,
        serving: &str,
        input: ImageTaskInput,
    ) -> R<AgentRequestDto> {
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        self.stage_image_generation(&scope.space_id, source, serving, input)
    }
}
