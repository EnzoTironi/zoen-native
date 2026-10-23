//! Recover an existing host run as an inert locator, never reconstruct a permit.
use super::*;
use client_core::image_tasks::ImageTaskDefinition;

pub(super) fn run_id(agent: &str, space: &str, request: &str) -> Result<String, RuntimeError> {
    if ![agent, space, request].into_iter().all(id) {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(format!(
        "image_run_{}",
        hash(
            &serde_json::to_vec(&("native-image-run/1", agent, space, request))
                .map_err(|_| RuntimeError::InvalidBinding)?
        )
    ))
}

impl RuntimeAuthority {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn image_host_run(
        &self,
        agent: &str,
        device: &str,
        space: &str,
        request: &str,
        definition: &ImageTaskDefinition,
        price: &ImagePriceProfile,
        gateway: GatewayConfig,
    ) -> Result<String, RuntimeError> {
        let run = run_id(agent, space, request)?;
        let trx = self.execution.transaction().await?;
        let existing = trx
            .get(&self.execution.run(&run, "image-record/1"), false)
            .await?
            .is_some();
        drop(trx);
        if !existing {
            let prepared = self
                .prepare_image_task(agent, device, request, price.clone(), gateway)
                .await?;
            if prepared != run {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(run);
        }
        let model = ModelGateway::new(gateway).map_err(|_| RuntimeError::InvalidProfile)?;
        let (record, _) = self.execution.read_image_task(&run, &self.custody).await?;
        let task = &record.task;
        if task.request_id != request
            || task.binding.context.agent != agent
            || task.binding.context.device != device
            || task.binding.context.space != space
            || task.binding.context.definition_version != definition.version
            || task.binding.price.image()? != price
            || price.digest()? != definition.price_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let dispatch = model
            .preflight_image(&task.request)
            .map_err(|_| RuntimeError::InvalidProfile)?;
        if dispatch.descriptor != task.dispatch
            || dispatch.request_digest != task.binding.request_digest
        {
            return Err(RuntimeError::InvalidBinding);
        }
        self.image_host_financial(agent, device, &run).await?;
        Ok(run)
    }

    pub(crate) async fn image_host_financial(
        &self,
        agent: &str,
        device: &str,
        run: &str,
    ) -> Result<Option<FinancialState>, RuntimeError> {
        let (record, _) = self.execution.read_image_task(run, &self.custody).await?;
        let custody = self
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let credential = custody.credential(agent, device)?;
        if record.original.context.principal != credential.principal() {
            return Err(RuntimeError::InvalidBinding);
        }
        self.finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        self.finance
            .inspect(&record.task.binding.context.attempt_id)
            .await
    }
}
