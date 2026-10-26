//! One authenticated source turn and its existing continuation/receipt decisions.

use super::*;
use client_core::RequestStatus;

/// A deferred turn has performed no admission, journal reservation or effect.
/// It is scheduling, not an unknown provider/executor result or a retry attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Handled,
    Retry,
    Deferred,
}

pub(super) async fn turn(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
) -> Outcome {
    evaluate(e, cx, def, space, before, t, None).await.outcome
}

pub(super) async fn selected(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
    selection: &source_mode::Bound,
) -> source_mode::Attempt {
    if selection.dispatch(e, cx, space, t).is_err() {
        return source_mode::Attempt {
            outcome: Outcome::Retry,
            observed: None,
        };
    }
    evaluate(e, cx, def, space, before, t, Some(selection)).await
}

async fn evaluate(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
    selection: Option<&source_mode::Bound>,
) -> source_mode::Attempt {
    let mut attempt = source_mode::Attempt {
        outcome: Outcome::Retry,
        observed: None,
    };
    let handled = checked(e, cx, def, space, before, t, selection, &mut attempt).await;
    if attempt.outcome != Outcome::Deferred {
        attempt.outcome = if handled {
            Outcome::Handled
        } else {
            Outcome::Retry
        };
    }
    attempt
}

#[expect(
    clippy::too_many_arguments,
    reason = "normal turn inputs plus optional selector and its actual canonical observation"
)]
async fn checked(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
    selection: Option<&source_mode::Bound>,
    attempt: &mut source_mode::Attempt,
) -> bool {
    if selection.is_some_and(|selection| selection.check(e).is_err()) {
        return false;
    }
    if let EntryKind::ItemEdited {
        item_id, version, ..
    } = &t.kind
    {
        // A client booked or cancelled in a mini-app: tell the owner, once.
        return match e.booking_note(item_id.clone(), *version) {
            Some(n) if owner_note(e, cx, &t.id).is_none() => {
                let mut n: serde_json::Value = serde_json::from_str(&n).unwrap_or_default();
                n["booking"]["source"] = t.id.clone().into();
                tell_owner(e, cx, n, space, &t.id)
            }
            _ => true,
        };
    }
    let Some(text) = text_of(t) else {
        return true;
    };
    // Idempotent on the source: did an earlier run already answer this message?
    let tl = e.timeline(space.to_string()).unwrap_or_default();
    if tl
        .iter()
        .any(|m| m.author.id == cx.me && m.reply_to.as_ref().is_some_and(|q| q.id == t.id))
    {
        trace::stop(space, &t.id, trace::Stop::AlreadyReplied, None);
        return true;
    }
    if let Some(note) = owner_note(e, cx, &t.id) {
        // Told the owner already. A proposal's confirmation may still be owed to the chat
        // (the reply failed after the note went out): send it now, from the note.
        return match note["agent_proposal"]["confirmation"].as_str() {
            Some(c) => selected_reply(e, space, c, &t.id, selection),
            None => true,
        };
    }
    let scoped = match e.agent_artifact_request(space, &t.id, &t.author.id) {
        Ok(scope) => scope.is_some(),
        Err(_) => return false,
    };
    let history: Vec<Turn> = before
        .iter()
        .rev()
        // A scoped App request reads through its bounded native tool. Unrelated
        // conversation history must not become an implicit artifact read.
        .filter(|_| !scoped)
        .filter_map(|m| {
            text_of(m).map(|x| Turn {
                space: space.into(),
                from: m.author.id.clone(),
                text: x.to_string(),
            })
        })
        .take(HISTORY)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let inbound = Turn {
        space: space.into(),
        from: t.author.id.clone(),
        text: text.to_string(),
    };
    let tc = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: def,
    };
    let mut tools = SpaceTools::new(e, cx, space, &t.author.id, &t.id);
    tools.selection = selection;
    let done = tools.done();
    let pending = e.tool_approval_requests(space, &t.id);
    let resumed = pending.last();
    if selection.is_some() && resumed.is_none() {
        attempt.observed = Some(None);
    }
    let result = if let Some(request) = resumed {
        let stop = |category| trace::stop(space, &t.id, category, Some(&request.id));
        let Some(dto) = e.requests().into_iter().find(|r| r.id == request.id) else {
            stop(trace::Stop::RequestUnavailable);
            return true;
        };
        if selection.is_some() {
            attempt.observed = Some(Some((
                dto.id.clone(),
                dto.status,
                e.tool_review_pending_intent(&dto.id),
            )));
        }
        // Only an authenticated owner intent may move a pending/expired call
        // into preparation. It still cannot reach continuation or execution.
        let prepare_pending = matches!(dto.status, RequestStatus::Pending | RequestStatus::Stale)
            && e.tool_review_pending_intent(&dto.id).is_some();
        if !prepare_pending {
            if let Some(category) = trace::Stop::request_status(dto.status) {
                stop(category);
                return true;
            }
        }
        let Some(proposal) = &request.tool_proposal else {
            stop(trace::Stop::InvalidContinuation);
            return true;
        };
        if proposal.schema == 1 && tools.deckcraft.is_none() {
            stop(trace::Stop::ExecutorUnavailable);
            return true;
        }
        let ask: ToolAsk = match serde_json::from_str(&proposal.continuation_json) {
            Ok(ask) => ask,
            Err(_) => {
                stop(trace::Stop::InvalidContinuation);
                return true;
            }
        };
        match tools.prepare_review(cx, tc, &inbound, request, &ask).await {
            Ok(
                space_tools::renewal::Preparation::Unrequested
                | space_tools::renewal::Preparation::Recovered,
            ) => {}
            Ok(space_tools::renewal::Preparation::Deferred) => {
                // Only the known pre-IO readiness gate may clear a scheduling
                // observation. No model/IO/attempt has started on this path.
                attempt.outcome = Outcome::Deferred;
                return false;
            }
            Ok(space_tools::renewal::Preparation::Pending) => {
                stop(trace::Stop::Pending);
                return true;
            }
            Ok(space_tools::renewal::Preparation::Reconcile) | Err(_) => {
                // This is an explanation of a refused continuation, never a receipt
                // or an implicit permission to repeat an ambiguous operation.
                stop(trace::Stop::RecoveryRefused);
                return selected_reply(
                    e,
                    space,
                    "I cannot safely renew this action. Reconcile its result and request a new proposal if needed; I have not repeated it.",
                    &t.id,
                    selection,
                );
            }
        }
        // Preparation is not approval, including when a receipt was reconciled
        // or the intent changed during the read-only synchronization preflight.
        if let Some(category) = trace::Stop::request_status(dto.status) {
            stop(category);
            return true;
        }
        let checkpoint = match tools.recovery_checkpoint(&request.id, &request.content_hash) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                turn_note(
                    selection,
                    "agent_host_source_checkpoint_refused",
                    format_args!("continuation checkpoint refused: {error}"),
                );
                stop(trace::Stop::CheckpointRefused);
                return true;
            }
        };
        let recovered = if let Some((mut progress, prepared)) = checkpoint {
            if let Some(prepared) = prepared {
                if prepared.envelope != ask.envelope
                    || prepared.policy_sha256 != ask.policy_sha256
                    || prepared.round < progress.round
                    || prepared.steps != progress.steps
                    || tools
                        .tools()
                        .iter()
                        .find(|definition| definition.name == prepared.call.name)
                        != Some(&prepared.definition)
                {
                    say("prepared continuation changed");
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
                progress = ToolProgress {
                    round: prepared.round,
                    steps: prepared.steps,
                    remaining: prepared.remaining,
                };
                progress.steps.push(ToolStep {
                    call: prepared.call,
                    result: String::new(),
                    is_error: false,
                });
            }
            let encoded = match serde_json::to_string(&progress.steps) {
                Ok(encoded) => encoded,
                Err(_) => {
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
            };
            let verified = match tools.recover_canonical(&request.id, proposal.schema, &encoded) {
                Ok(results) => results,
                Err(_) => {
                    say(
                        "completed continuation is stale, unauthorized or lacks a canonical receipt",
                    );
                    stop(trace::Stop::RecoveryRefused);
                    return true;
                }
            };
            progress.steps = match serde_json::from_str(&verified) {
                Ok(steps) => steps,
                Err(_) => {
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
            };
            Some(progress)
        } else {
            None
        };
        // Both fresh execution and completed-result recovery retain the same
        // persistent operational attempt budget. This is not a provider meter.
        match tools.begin_resume(&request.id, &request.content_hash) {
            Ok(true) => {}
            Ok(false) => {
                say("reviewed action reached its continuation retry limit");
                stop(trace::Stop::AttemptLimit);
                return true;
            }
            Err(error) => {
                turn_note(
                    selection,
                    "agent_host_source_reservation_refused",
                    format_args!("continuation reservation refused: {error}"),
                );
                stop(trace::Stop::ReservationRefused);
                return true;
            }
        }
        if let Some(progress) = recovered {
            trace::progress(space, &t.id, &request.id, &progress);
            cx.runtime
                .resume_tool_progress(tc, &history, &inbound, &tools, ask, progress)
                .await
        } else {
            let permit =
                match page_tools::CanonicalPermit::new(&tools, &request.id, proposal.schema) {
                    Ok(permit) => permit,
                    Err(refusal) => {
                        trace::admission::refused(space, &t.id, &request.id, refusal);
                        say("reviewed action is stale or no longer authorized");
                        stop(trace::Stop::ApprovalRefused);
                        return true;
                    }
                };
            let admission = CanonicalToolAdmission {
                tools: &tools,
                permit,
            };
            trace::approved(space, &t.id, &request.id, &ask);
            cx.runtime
                .resume_tool_ask(tc, &history, &inbound, &tools, ask, &admission)
                .await
        }
    } else if let Ok(Some(outcome)) =
        JournalState::known_turn(cx.journal.as_path(), cx, space, t, def)
    {
        Ok(outcome)
    } else {
        let source_host = source_recovery::SourceTools::new(&tools, cx, def, t);
        let recovered = match source_host.recover() {
            Ok(recovered) => recovered,
            Err(error) => {
                turn_note(
                    selection,
                    "agent_host_source_checkpoint_refused",
                    format_args!("source recovery refused: {error}"),
                );
                return false;
            }
        };
        let result = if let Some((original, progress)) = recovered {
            cx.runtime
                .resume_tool_progress(tc, &history, &inbound, &source_host, original, progress)
                .await
        } else {
            match JournalState::begin_turn(cx.journal.as_path(), cx, space, t) {
                Ok(true) => {}
                Ok(false) => {
                    trace::stop(space, &t.id, trace::Stop::SourceAttemptLimit, None);
                    return true;
                }
                Err(_) => {
                    trace::stop(space, &t.id, trace::Stop::SourceReservationRefused, None);
                    return false;
                }
            }
            trace::normal(space, &t.id, &done);
            cx.runtime
                .turn_with_tools(tc, &history, &inbound, &source_host, done)
                .await
        };
        if matches!(&result, Err(zoen_agentd::HostError::BrainNotDispatched(_))) {
            if tools.check_selection().is_err() {
                return false;
            }
            if let Err(error) =
                JournalState::model_not_dispatched(cx.journal.as_path(), cx, space, t)
            {
                turn_note(
                    selection,
                    "agent_host_source_reservation_refused",
                    format_args!("model witness refused: {error}"),
                );
                return false;
            }
        }
        if tools.check_selection().is_err() {
            return false;
        }
        if let Ok(outcome) = &result {
            if let Err(error) =
                JournalState::model_known(cx.journal.as_path(), cx, space, t, outcome, def)
            {
                turn_note(
                    selection,
                    "agent_host_source_checkpoint_refused",
                    format_args!("model result checkpoint refused: {error}"),
                );
                return false;
            }
        }
        result
    };
    if tools.check_selection().is_err() {
        return false;
    }
    trace::turn_result(space, &t.id, &result);
    let out = match result {
        Ok(o) => o,
        Err(err) => {
            turn_note(
                selection,
                "agent_host_source_runtime_failed",
                format_args!("turn failed: {err}"),
            );
            return false;
        }
    };
    match out {
        TurnOutcome::Silence => true,
        TurnOutcome::Blocked { reason } => {
            turn_note(
                selection,
                "agent_host_source_blocked",
                format_args!("blocked: {reason}"),
            );
            true
        }
        TurnOutcome::Say { text } if scoped => e
            .agent_artifact_reply(space, &t.id, &t.author.id, &text)
            .is_ok(),
        TurnOutcome::Say { text } => selected_reply(e, space, &text, &t.id, selection),
        TurnOutcome::ToolAsk { ask } if ask.call.name == image_tools::TOOL => {
            tools.image_stage(&ask.call).is_ok()
        }
        TurnOutcome::ToolAsk { ask } => match tools.stage_approval(&ask) {
            Ok(dto) => {
                trace::staged(space, &t.id, &ask, &dto);
                true
            }
            Err(error) => {
                trace::delivery(space, &t.id, trace::Delivery::ApprovalStaging, false);
                turn_note(
                    selection,
                    "agent_host_source_approval_refused",
                    format_args!("tool approval refused: {error}"),
                );
                true
            }
        },
        // A scoped draft stays at the signed source, not another conversation.
        TurnOutcome::AskOwner { draft, .. } if scoped => e
            .agent_artifact_reply(space, &t.id, &t.author.id, &draft)
            .is_ok(),
        TurnOutcome::AskOwner { draft, rule } => selected_owner_note(
            e,
            cx,
            json!({"ask_owner": {"space": space, "source": t.id, "from": t.author.id,
                                 "draft": draft, "rule": rule}}),
            space,
            &t.id,
            selection,
        ),
        TurnOutcome::ProposeAgent { .. } if scoped => false,
        TurnOutcome::ProposeAgent { spec, confirmation } => {
            selected_proposal(e, cx, space, t, &spec, confirmation.as_deref(), selection)
        }
    }
}
