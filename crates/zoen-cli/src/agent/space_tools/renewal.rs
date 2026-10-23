//! A signed owner request prepares another review; it never replays unknown IO.
use super::*;

pub(in crate::agent) enum Preparation {
    Unrequested,
    Deferred,
    Pending,
    Recovered,
    Reconcile,
}

impl SpaceTools<'_> {
    pub(super) fn register_staged(
        &self,
        dto: client_core::AgentRequestDto,
        prior: &[types::AgentRequest],
    ) -> Result<client_core::AgentRequestDto, String> {
        if !prior.iter().any(|r| r.id == dto.id) {
            let hash = &dto
                .tool_approval
                .as_ref()
                .ok_or("Missing held request")?
                .content_hash;
            JournalState::register_hold(self.journal, &dto.id, hash, self.source)?;
        }
        Ok(dto)
    }

    pub(in crate::agent) fn journal_binding(
        &self,
        request: &str,
        hash: &str,
    ) -> Result<(String, String), String> {
        self.check_selection()?;
        // Historical keys keep their encoding and behavior. Only authenticated
        // signed lineage can redirect operational metadata to a different key.
        let renewed = self
            .e
            .tool_approval_requests(&self.space, self.source)
            .iter()
            .any(|r| {
                r.id == request && r.tool_proposal.as_ref().is_some_and(|p| p.review.is_some())
            });
        if renewed {
            self.e
                .tool_review_journal_root(request, hash)
                .map_err(|e| e.to_string())
        } else {
            Ok((request.into(), hash.into()))
        }
    }

    pub(in crate::agent) fn entering_canonical_executor(&self) -> Result<(), String> {
        let Some((request, hash, _)) = self.canonical_continuation()? else {
            return Err("The canonical continuation is missing".into());
        };
        JournalState::entering_executor(self.journal, &request, &hash, self.source)
    }

    /// Every recovered result passes the existing native receipt/content checks.
    /// Empty results are probes, never a claim that a read or effect completed.
    fn recover_reviewed_result(&self, request: &types::AgentRequest, ask: &ToolAsk) -> bool {
        let Some(proposal) = request.tool_proposal.as_ref() else {
            return false;
        };
        let result = self.done().into_iter().find(|s| s.call == ask.call);
        let mut steps = ask.steps.clone();
        steps.push(result.unwrap_or_else(|| ToolStep {
            call: ask.call.clone(),
            result: String::new(),
            is_error: false,
        }));
        let Ok(encoded) = serde_json::to_string(&steps) else {
            return false;
        };
        let Ok(verified) = self.recover_canonical(&request.id, proposal.schema, &encoded) else {
            return false;
        };
        let Ok(steps) = serde_json::from_str(&verified) else {
            return false;
        };
        self.checkpoint_continuation(&ToolProgress {
            round: ask.round,
            steps,
            remaining: ask.remaining.clone(),
        })
        .is_ok()
    }

    pub(in crate::agent) async fn prepare_review(
        &self,
        cx: &Ctx,
        context: TurnContext<'_>,
        inbound: &Turn,
        request: &types::AgentRequest,
        ask: &ToolAsk,
    ) -> Result<Preparation, String> {
        let Some(intent) = self.e.tool_review_pending_intent(&request.id) else {
            return Ok(Preparation::Unrequested);
        };
        if self.e.wait_until_settled(0).await.is_err() {
            // Readiness is observed before any reservation, provider or native IO.
            // Keep the signed intent pending under the existing outer host deadline.
            self.check_selection()?;
            return Ok(Preparation::Deferred);
        }
        self.check_selection()?;
        self.prepare_settled_review(cx, context, inbound, request, ask, &intent)
    }

    // The normal entrypoint above enforces settled synchronization. This cohesive
    // synchronous half is also exercised by real signed offline fixtures.
    pub(in crate::agent) fn prepare_settled_review(
        &self,
        cx: &Ctx,
        context: TurnContext<'_>,
        inbound: &Turn,
        request: &types::AgentRequest,
        ask: &ToolAsk,
        intent: &str,
    ) -> Result<Preparation, String> {
        self.check_selection()?;
        if self.e.tool_review_pending_intent(&request.id).as_deref() != Some(intent) {
            return Err("The owner review intent changed".into());
        }
        if !cx
            .runtime
            .validate_tool_review(context, inbound, self, ask)
            .map_err(|_| "The held runtime context changed")?
        {
            return Ok(Preparation::Reconcile);
        }
        let (root, hash) = self.journal_binding(&request.id, &request.content_hash)?;
        let never_started = JournalState::renewal_safe(self.journal, &root, &hash, self.source)?;
        if never_started {
            let staged = self
                .e
                .stage_tool_review_successor(&request.id, intent)
                .map_err(|_| "The renewed review context changed")?;
            self.check_selection()?;
            if staged.is_some() {
                return Ok(Preparation::Pending);
            }
        }
        // Known completed checkpoints use the original recovery path. Unknown
        // outcomes can only become completed through an exact native receipt.
        if self
            .recovery_checkpoint(&request.id, &request.content_hash)
            .is_ok_and(|p| p.is_some())
            || self.recover_reviewed_result(request, ask)
        {
            return Ok(Preparation::Recovered);
        }
        Ok(Preparation::Reconcile)
    }
}
