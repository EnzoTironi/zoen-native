//! Trusted conversation consumer over the existing native and financial authority.
use crate::*;

/// Operator configuration, never deserialized from a model tool call.
pub struct ReplyHostConfig {
    pub agent: String,
    pub device: String,
    pub target: ImageDeliveryTarget,
}

pub struct ReplyRuntimeHost {
    runtime: Arc<RuntimeAuthority>,
    gate: Arc<gate::Gate>,
    config: ReplyHostConfig,
}

#[derive(Debug)]
pub enum ReplyHostProgress {
    CatchingUp {
        run: String,
        financial: Option<FinancialState>,
    },
    /// An observation of absence is not a zero-cost or non-dispatch witness.
    Held {
        run: String,
        audit: gate::AuditRecord,
        financial: Option<FinancialState>,
    },
    AwaitingRecovery {
        run: String,
        financial: FinancialState,
    },
    Output {
        run: String,
        output: ReplyOutputInspection,
        delivery: ReplyReceiptProgress,
        financial: FinancialState,
        audit: gate::AuditRecord,
    },
    Completed {
        run: String,
        output: ReplyOutputInspection,
        delivery: ReplyReceiptProgress,
    },
}

impl ReplyRuntimeHost {
    pub fn configured(
        runtime: Arc<RuntimeAuthority>,
        config: ReplyHostConfig,
        gate: gate::Gate,
    ) -> Result<Self, RuntimeError> {
        if !id(&config.agent) || !id(&config.device) {
            return Err(RuntimeError::InvalidProfile);
        }
        let scope = runtime.production_output_scope()?;
        scope.check_context(&runtime)?;
        scope.check_device(&config.agent, &config.device)?;
        scope.check_target(&config.target)?;
        Ok(Self {
            runtime,
            gate: Arc::new(gate),
            config,
        })
    }

    /// Owner-signed budget, scoped to this host's certified current owner.
    /// Uses the same directory locks and retained installation as other hosts.
    pub async fn import_owner_policy(
        &self,
        signed: types::owner_budget::SignedOwnerPolicy,
    ) -> Result<OwnerPolicyInstallation, RuntimeError> {
        self.runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .install_owner_policy(
                &self.runtime,
                &self.config.agent,
                &self.config.device,
                signed,
            )
            .await
    }

    pub async fn owner_policy_options(
        &self,
        signed: &types::owner_policy_context::SignedOwnerPolicyInquiry,
    ) -> Result<types::owner_policy_context::OwnerPolicyOptions, OwnerPolicyContextFailure> {
        self.runtime
            .native
            .as_ref()
            .ok_or(
                OwnerPolicyContextStage::Credential.failed(RuntimeError::CoreAuthorityUnavailable),
            )?
            .owner_policy_options(
                &self.runtime,
                &self.config.agent,
                &self.config.device,
                &crate::pricing::BoundPrice::Chat(self.runtime.price.clone()),
                None,
                signed,
            )
            .await
    }

    pub fn configured_device(&self) -> &str {
        &self.config.device
    }

    /// Read one bounded tool page from the host-bound source conversation.
    /// Neither arguments nor a run locator grant access or authorize model spend.
    pub async fn search_history(
        &self,
        run: &str,
        arguments: &serde_json::Value,
    ) -> Result<client_core::MessageSearchPage, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let scope = self.runtime.production_output_scope()?;
        scope.check_context(&self.runtime)?;
        scope.check_device(&self.config.agent, &self.config.device)?;
        scope.check_target(&self.config.target)?;
        let native = self
            .runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let state = native
            .reply_host_state(
                &self.runtime,
                run,
                scope,
                (&self.config.agent, &self.config.device),
            )
            .await?;
        if !self.refresh(&state.space).await? {
            return Err(RuntimeError::Unavailable);
        }
        native
            .search_reply_history(&self.runtime, run, arguments, scope)
            .await
    }

    /// Canonical owner lifecycle revision, never an execution permit.
    pub async fn control_revision(&self) -> Result<Option<u64>, RuntimeError> {
        self.runtime.check_reply_continuity().await?;
        self.runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?
            .reply_control_revision(&self.runtime, &self.config.agent, &self.config.device)
            .await
    }

    /// Bounded current membership hints for this certified host only.
    pub async fn conversation_page(
        &self,
        after: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), RuntimeError> {
        self.runtime.check_reply_continuity().await?;
        let native = self
            .runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        native
            .conversation_page(
                &self.runtime,
                &self.config.agent,
                &self.config.device,
                after,
            )
            .await
    }

    pub async fn pending_page(&self, after: Option<&str>) -> Result<ReplyWorkPage, RuntimeError> {
        self.runtime.check_reply_continuity().await?;
        self.runtime
            .execution
            .reply_page_scoped(
                16,
                after,
                &self.runtime.custody,
                Some((&self.config.agent, &self.config.device)),
            )
            .await
    }

    /// Canonical relay hint routing, never a dispatch permit.
    pub fn wake_subject(&self) -> Result<String, RuntimeError> {
        zoen_relay::fanout::worker_subject(
            self.runtime
                .execution
                .source
                .as_ref()
                .ok_or(RuntimeError::CoreAuthorityUnavailable)?
                .cell(),
            &self.config.agent,
        )
        .ok_or(RuntimeError::InvalidBinding)
    }

    pub async fn discover(&self, space: &str) -> Result<ReplySync, RuntimeError> {
        let scope = self.runtime.production_output_scope()?;
        scope.check_context(&self.runtime)?;
        scope.check_device(&self.config.agent, &self.config.device)?;
        scope.check_target(&self.config.target)?;
        self.runtime
            .sync_reply_runs(&self.config.agent, &self.config.device, space)
            .await
    }

    /// Recover one original; neither retained text nor a Gate decision replaces
    /// current native authorization, the frozen binding or one-shot admission.
    pub async fn advance(&self, run: &str) -> Result<ReplyHostProgress, RuntimeError> {
        if !id(run) {
            return Err(RuntimeError::InvalidBinding);
        }
        let runtime = &self.runtime;
        let scope = runtime.production_output_scope()?;
        scope.check_context(runtime)?;
        scope.check_device(&self.config.agent, &self.config.device)?;
        scope.check_target(&self.config.target)?;
        let native = runtime
            .native
            .as_ref()
            .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
        let state = native
            .reply_host_state(
                runtime,
                run,
                scope,
                (&self.config.agent, &self.config.device),
            )
            .await?;
        if let Some(next) = runtime
            .execution
            .next_search_reply(run, &runtime.custody)
            .await?
        {
            return Box::pin(self.advance(&next)).await;
        }
        if let Some(output) = state.output {
            let delivery = runtime.reconcile_reply_receipt(run).await?;
            match delivery {
                ReplyReceiptProgress::Accepted { .. } | ReplyReceiptProgress::Refused { .. } => {
                    return Ok(ReplyHostProgress::Completed {
                        run: run.into(),
                        output,
                        delivery,
                    });
                }
                ReplyReceiptProgress::CatchingUp { .. } => {
                    return self.catching_up(run, &state.attempt).await
                }
                ReplyReceiptProgress::Waiting => {}
            }
        }
        if !self.refresh(&state.space).await? {
            return self.catching_up(run, &state.attempt).await;
        }
        if !state.captured {
            let decision = native
                .check_reply_input(runtime, run, scope, self.gate.clone())
                .await?;
            if decision.outcome != gate::Outcome::Allow {
                return Ok(ReplyHostProgress::Held {
                    run: run.into(),
                    audit: decision.audit,
                    financial: runtime.inspect_attempt(&state.attempt).await?,
                });
            }
            // A remote policy provider may race revocation or source changes.
            if !self.refresh(&state.space).await? {
                return self.catching_up(run, &state.attempt).await;
            }
        }
        let financial = match Box::pin(runtime.run_model(run)).await {
            Ok(retained) => retained.financial,
            Err(RuntimeError::Unavailable) => {
                if let Some(financial) = runtime.inspect_attempt(&state.attempt).await? {
                    return Ok(ReplyHostProgress::AwaitingRecovery {
                        run: run.into(),
                        financial,
                    });
                }
                return Err(RuntimeError::Unavailable);
            }
            Err(error) => return Err(error),
        };
        if let Some(next) = Box::pin(native.continue_search_reply(runtime, run, scope)).await? {
            return Box::pin(self.advance(&next)).await;
        }
        let review = match native
            .review_reply_output(runtime, run, scope, self.gate.clone())
            .await?
        {
            native::ReplyOutputCheck::Allowed(review) => review,
            native::ReplyOutputCheck::Held(audit) => {
                return Ok(ReplyHostProgress::Held {
                    run: run.into(),
                    audit,
                    financial: Some(financial),
                })
            }
            native::ReplyOutputCheck::Unusable => {
                return Ok(ReplyHostProgress::AwaitingRecovery {
                    run: run.into(),
                    financial,
                })
            }
        };
        if !self.refresh(&state.space).await? {
            return self.catching_up(run, &state.attempt).await;
        }
        let output = Box::pin(native.prepare_reviewed_output(runtime, run, scope, &review)).await?;
        let delivery =
            Box::pin(runtime.deliver_reply_output(run, self.config.target.clone())).await?;
        Ok(ReplyHostProgress::Output {
            run: run.into(),
            output,
            delivery,
            financial,
            audit: review.audit().clone(),
        })
    }

    async fn refresh(&self, space: &str) -> Result<bool, RuntimeError> {
        let progress = self.discover(space).await?;
        Ok(progress.caught_up && progress.discovery_complete)
    }

    async fn catching_up(
        &self,
        run: &str,
        attempt: &str,
    ) -> Result<ReplyHostProgress, RuntimeError> {
        Ok(ReplyHostProgress::CatchingUp {
            run: run.into(),
            financial: self.runtime.inspect_attempt(attempt).await?,
        })
    }
}
