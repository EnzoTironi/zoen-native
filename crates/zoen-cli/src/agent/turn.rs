//! One authenticated source turn and its existing continuation/receipt decisions.

use super::*;

/// One message. `true` once it is handled: replied, or deliberately silent or blocked.
pub(super) async fn turn(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
) -> bool {
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
            Some(c) => reply(e, space, c, &t.id),
            None => true,
        };
    }
    let history: Vec<Turn> = before
        .iter()
        .rev()
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
    let tools = SpaceTools::new(e, cx, space, &t.author.id, &t.id);
    let done = tools.done();
    let pending = e.tool_approval_requests(space, &t.id);
    let resumed = pending.last();
    let result = if let Some(request) = resumed {
        let stop = |category| trace::stop(space, &t.id, category, Some(&request.id));
        let Some(dto) = e.requests().into_iter().find(|r| r.id == request.id) else {
            stop(trace::Stop::RequestUnavailable);
            return true;
        };
        if let Some(category) = trace::Stop::request_status(dto.status) {
            stop(category);
            return true;
        }
        let Some(worker) = tools.deckcraft else {
            stop(trace::Stop::ExecutorUnavailable);
            return true;
        };
        let Some(proposal) = &request.tool_proposal else {
            stop(trace::Stop::InvalidContinuation);
            return true;
        };
        let ask: ToolAsk = match serde_json::from_str(&proposal.continuation_json) {
            Ok(ask) => ask,
            Err(_) => {
                stop(trace::Stop::InvalidContinuation);
                return true;
            }
        };
        let checkpoint = match tools.recovery_checkpoint(&request.id, &request.content_hash) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                say(format_args!("continuation checkpoint refused: {error}"));
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
            let verified = match e.recover_tool_results(
                &request.id,
                worker.binding_sha256(),
                &encoded,
            ) {
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
                say(format_args!("continuation reservation refused: {error}"));
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
            let permit = match e.approved_tool_turn(&request.id, worker.binding_sha256()) {
                Ok(permit) => permit,
                Err(_) => {
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
    } else {
        trace::normal(space, &t.id, &done);
        cx.runtime
            .turn_with_tools(tc, &history, &inbound, &tools, done)
            .await
    };
    trace::turn_result(space, &t.id, &result);
    let out = match result {
        Ok(o) => o,
        Err(err) => {
            say(format_args!("turn failed: {err}"));
            return false;
        }
    };
    match out {
        TurnOutcome::Silence => true,
        TurnOutcome::Blocked { reason } => {
            say(format_args!("blocked: {reason}"));
            true
        }
        TurnOutcome::Say { text } => reply(e, space, &text, &t.id),
        TurnOutcome::ToolAsk { ask } => match tools.stage_approval(&ask) {
            Ok(dto) => {
                trace::staged(space, &t.id, &ask, &dto);
                true
            }
            Err(error) => {
                trace::delivery(space, &t.id, trace::Delivery::ApprovalStaging, false);
                say(format_args!("tool approval refused: {error}"));
                true
            }
        },
        TurnOutcome::AskOwner { draft, rule } => tell_owner(
            e,
            cx,
            json!({"ask_owner": {"space": space, "source": t.id, "from": t.author.id,
                                 "draft": draft, "rule": rule}}),
            space,
            &t.id,
        ),
        TurnOutcome::ProposeAgent { spec, confirmation } => {
            propose(e, cx, space, t, &spec, confirmation.as_deref())
        }
    }
}
