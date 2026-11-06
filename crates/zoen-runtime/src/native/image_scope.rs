//! Operation-scoped authority uses the qualified image or reply route.
//! Neither retained locators nor caller flags construct live permission.
use super::*;

impl RuntimeAuthority {
    /// Qualify only Images on this exact operator route. No provider call or
    /// publication occurs: the live socket proves legacy binding is refused
    /// and the managed, privately paired checkpoint binding is actually served.
    pub async fn qualify_managed_images_output(
        mut self,
        target: ImageDeliveryTarget,
    ) -> Result<Self, RuntimeError> {
        if self.image_output.is_some() {
            return Err(RuntimeError::InvalidBinding);
        }
        self.image_output = Some(ManagedNativeOutput::qualify(&self, target).await?);
        Ok(self)
    }

    pub(crate) fn production_image_output_scope(
        &self,
    ) -> Result<&dyn NativeOutputAuthority, RuntimeError> {
        self.image_output
            .as_ref()
            .map(|scope| scope as &dyn NativeOutputAuthority)
            .ok_or(RuntimeError::CoreAuthorityUnavailable)
    }
    pub(crate) fn image_output_scope(&self) -> Result<&dyn NativeOutputAuthority, RuntimeError> {
        if let Some(scope) = &self.image_output {
            return Ok(scope);
        }
        #[cfg(test)]
        if let Some(scope) = &self.output_fixture {
            return Ok(scope);
        }
        Err(RuntimeError::CoreAuthorityUnavailable)
    }
    pub(crate) fn image_continuity(&self) -> Result<(), RuntimeError> {
        self.image_output_scope()?.check_context(self)
    }
    pub(crate) fn step_continuity(&self, step: &crate::VerifiedStep) -> Result<(), RuntimeError> {
        match step {
            crate::VerifiedStep::NativeImage(_) => self.image_continuity(),
            crate::VerifiedStep::Native(_) => self.reply_continuity(),
            #[cfg(test)]
            crate::VerifiedStep::Fixture(step)
                if step.request.operation == zoen_models::Operation::ImageGeneration =>
            {
                self.image_continuity()
            }
            #[cfg(test)]
            crate::VerifiedStep::Fixture(_) => self.continuity(),
        }
    }
}
