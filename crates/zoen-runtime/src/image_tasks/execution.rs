use super::*;
use async_trait::async_trait;
use capture::ImageCapture;
#[async_trait]
pub(crate) trait ImageAttemptAdapter: Send + Sync {
    async fn generate(
        &self,
        request: ImageRequest,
        authority: &dyn DispatchAuthority,
    ) -> Result<ImageAttemptResult, zoen_models::GatewayError>;
}
#[async_trait]
impl ImageAttemptAdapter for ModelGateway {
    async fn generate(
        &self,
        request: ImageRequest,
        authority: &dyn DispatchAuthority,
    ) -> Result<ImageAttemptResult, zoen_models::GatewayError> {
        self.generate_image(request, authority).await
    }
}

impl RuntimeAuthority {
    /// Deliver an already-prepared original. The same production output and
    /// continuity gates remain closed; routing configuration grants no permit.
    pub async fn deliver_image_task_output(
        &self,
        run: &str,
        target: crate::ImageDeliveryTarget,
    ) -> Result<crate::ImageDeliveryProgress, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.continuity()?;
        let scope = self.production_output_scope()?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .deliver_image_output(self, run, &target, scope)
            .await
    }
    pub(crate) fn image_output_scope(&self) -> Result<&native::FreshOutputNamespace, RuntimeError> {
        #[cfg(test)]
        if let Some(scope) = self.output_fixture.as_ref() {
            return Ok(scope);
        }
        self.production_output_scope()
    }

    /// A host supplies locators and configured prices, never native facts or a
    /// caller-built binding. Preparing a run grants no paid dispatch capability.
    pub async fn prepare_image_task(
        &self,
        agent: &str,
        device: &str,
        request: &str,
        price: ImagePriceProfile,
        gateway: GatewayConfig,
    ) -> Result<String, RuntimeError> {
        if ![agent, device, request].into_iter().all(id) {
            return Err(RuntimeError::InvalidBinding);
        }
        price.digest()?;
        let gateway = ModelGateway::new(gateway).map_err(|_| RuntimeError::InvalidProfile)?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .prepare_image_run(self, agent, device, request, &price, &gateway)
            .await
    }
    /// Production remains closed until BOTH managed restore continuity and the
    /// existing non-replayable output barrier are qualified. No host flag opens it.
    pub async fn run_image_task(
        &self,
        run: &str,
        gateway: GatewayConfig,
    ) -> Result<RetainedModel, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.continuity()?;
        let scope = self.production_output_scope()?;
        scope
            .check_in(&self.execution, &self.execution.transaction().await?)
            .await?;
        let gateway = ModelGateway::new(gateway).map_err(|_| RuntimeError::InvalidProfile)?;
        let image = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .verified_image(self, run, &gateway)
            .await?;
        let task = image.task().clone();
        let step = VerifiedStep::NativeImage(Box::new(image));
        let result = self.complete_image_task(&step, &task, &gateway).await;
        let _ = step.release(&self.execution).await;
        result.map(|financial| RetainedModel { financial })
    }
    pub async fn prepare_image_task_output(
        &self,
        run: &str,
        gateway: GatewayConfig,
    ) -> Result<ImageOutputInspection, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        self.continuity()?;
        let scope = self.production_output_scope()?;
        scope
            .check_in(&self.execution, &self.execution.transaction().await?)
            .await?;
        let gateway = ModelGateway::new(gateway).map_err(|_| RuntimeError::InvalidProfile)?;
        self.native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .prepare_image_output(self, run, &gateway, scope)
            .await
    }
    pub(crate) async fn complete_image_task(
        &self,
        step: &VerifiedStep,
        task: &ImageTask,
        gateway: &dyn ImageAttemptAdapter,
    ) -> Result<FinancialState, RuntimeError> {
        self.continuity()?;
        task.validate()?;
        if step.request().context != task.binding.context
            || step.request().operation != zoen_models::Operation::ImageGeneration
            || step.request().messages
                != vec![zoen_models::InputMessage::User {
                    text: task.request.prompt.clone(),
                }]
            || !step.request().tools.is_empty()
            || step.request().max_output_tokens != 0
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let session = Session {
            runtime: self,
            step,
            admitted: Mutex::new(None),
            image: Some(task),
        };
        let result = gateway
            .generate(task.request.clone(), &session)
            .await
            .map_err(|error| match error {
                zoen_models::GatewayError::Admission(AdmissionError::Denied) => {
                    RuntimeError::Denied
                }
                _ => RuntimeError::Unavailable,
            })?;
        let admitted = session
            .admitted
            .lock()
            .map_err(|_| RuntimeError::Unavailable)?
            .take()
            .ok_or(RuntimeError::InvalidBinding)?;
        if admitted.binding != task.binding {
            return Err(RuntimeError::InvalidBinding);
        }
        self.retain_image_capture(ImageCapture {
            task: task.clone(),
            result,
            nonce: admitted.nonce,
        })
        .await
    }
    async fn retain_image_capture(
        &self,
        capture: ImageCapture,
    ) -> Result<FinancialState, RuntimeError> {
        let plain = zeroize::Zeroizing::new(capture.encode()?);
        let attempt = &capture.task.binding.context.attempt_id;
        let (digest, sealed) = self.custody.seal_bounded_bytes(
            &format!("evidence/{attempt}"),
            &plain,
            capture::MAXIMUM,
        )?;
        #[cfg(test)]
        let associate = self.fault.load(Ordering::SeqCst) != 40;
        #[cfg(not(test))]
        let associate = true;
        if associate {
            let _ = self
                .execution
                .associate_image_capture(&capture.task, &capture.nonce, &digest, &self.custody)
                .await;
        }
        let charge = capture.task.retained_charge(&capture.result)?;
        // Either existing durable store can retain incurred cost independently
        // of output or a later device/grant/lease loss. Unknown usage keeps hold.
        let sql = self
            .finance
            .settle_capture(&capture.task.binding, charge, &digest, &sealed)
            .await;
        let fdb = self
            .execution
            .retain_evidence(attempt, &digest, &sealed)
            .await;
        if sql.is_err() && fdb.is_err() {
            return Err(RuntimeError::Unavailable);
        }
        Ok(sql.unwrap_or(FinancialState::Claimed))
    }
}
