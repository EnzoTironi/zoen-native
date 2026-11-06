//! Public execution uses the qualified reply route and original native admission.
use crate::*;
impl RuntimeAuthority {
    pub async fn run_model(&self, run: &str) -> Result<RetainedModel, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        if self.native.is_none() {
            return Err(RuntimeError::CoreAuthorityUnavailable);
        }
        self.check_reply_continuity().await?;
        if self.runtime_continuity.is_some() {
            if let Some(financial) = native::retained_model_state(self, run).await? {
                return Ok(RetainedModel { financial });
            }
        }
        #[cfg(test)]
        println!("native paid stage: verified_step_start");
        let step = Box::pin(self.verified_step(run)).await?;
        #[cfg(test)]
        println!("native paid stage: verified_step_complete");
        let result = Box::pin(self.complete_verified(&step)).await;
        #[cfg(test)]
        println!(
            "native paid stage: complete_verified result={:?}",
            result.as_ref().map(|_| "ok")
        );
        // Lease loss must not erase an already incurred and retained bill.
        let _ = step.release(&self.execution).await;
        result.map(|(_, financial)| RetainedModel { financial })
    }
}
