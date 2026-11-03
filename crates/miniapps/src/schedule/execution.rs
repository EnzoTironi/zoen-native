//! Agenda execution.

use super::{
    booking_index, check_slot, clean, configure_fields, flag_outside_hours, has_open, is_for, json,
    next_id, note, occupies, push_event, push_log, resolve_events, role, s, service_id, t, tr,
    trim, when_label, Actor, Applied, Role, Value, MAX_OPEN_PER_MEMBER, MINUTE,
};

/// A booking the agent already made for this `ref` and slot, still standing: what to
/// tell the client again.
pub fn replayed(tool_name: &str, args: &Value, st: &Value) -> Option<String> {
    let r = args["ref"]
        .as_str()
        .filter(|r| tool_name == "schedule_book" && !r.is_empty())?;
    let b = st["bookings"].as_array()?.iter().find(|b| {
        b["ref"].as_str() == Some(&clean(r, 64))
            && b["startMs"] == args["startMs"]
            && b["forId"] == args["forId"]
            && matches!(b["status"].as_str(), Some("pending" | "confirmed"))
    })?;
    let when = when_label(st, b["startMs"].as_i64().unwrap_or(0));
    Some(tr!(
        "Já pedido: {}, {when}. O agente confirma.",
        "Already requested: {}, {when}. The agent confirms.",
        b["serviceName"].as_str().unwrap_or_default()
    ))
}

/// A tool call. Every change bumps `rev`, so a write that two devices made from the same
/// state can be told apart from one made on top of the other (see [`reconcile`]).
pub fn apply(
    tool: &str,
    args: &Value,
    st: Value,
    actor: &Actor,
    now: i64,
) -> Result<Applied, String> {
    let rev = st["rev"].as_i64().unwrap_or(0);
    let mut a = apply_tool(tool, args, st, actor, now)?;
    if tool != "schedule_get_state" {
        a.state["rev"] = json!(rev + 1);
    }
    Ok(a)
}

pub(super) fn apply_tool(
    tool: &str,
    args: &Value,
    st: Value,
    actor: &Actor,
    now: i64,
) -> Result<Applied, String> {
    let who = role(&st, actor.id);
    match tool {
        "schedule_get_state" => Ok(Applied::new(st, String::new(), String::new())),
        "schedule_configure" => configure(args, st, actor, now, who),
        "schedule_book" => book(args, st, actor, now, who),
        "schedule_resolve" => resolve(args, st, actor, now, who),
        "schedule_cancel" => cancel(args, st, actor, now),
        other => Err(tr!(
            "schedule não tem a ferramenta {other}",
            "schedule has no tool {other}"
        )),
    }
}

fn configure(
    args: &Value,
    mut st: Value,
    actor: &Actor,
    now: i64,
    who: Role,
) -> Result<Applied, String> {
    if !matches!(who, Role::Owner | Role::Agent) {
        return Err(t(
            "só o dono ou o agente ajustam a agenda",
            "only the owner or the agent can configure the schedule",
        ));
    }
    if args.get("resolvers").is_some() && who != Role::Owner {
        return Err(t(
            "só o dono escolhe quem resolve pedidos",
            "only the owner picks who resolves requests",
        ));
    }
    configure_fields(&mut st, args)?;
    let flagged = flag_outside_hours(&mut st, actor.id, now);
    push_log(
        &mut st,
        actor.name,
        &t("ajustou a agenda", "updated the schedule"),
        now,
    );
    trim(&mut st, now);
    let mut a = Applied::new(
        st,
        tr!("{} ajustou a agenda", "{} updated the schedule", actor.name),
        t("Agenda atualizada.", "Schedule updated."),
    );
    a.title = a.state["title"].as_str().map(String::from);
    if !flagged.is_empty() {
        let list = flagged.join("; ");
        a.text = tr!(
            "Agenda atualizada. Ficaram fora do novo horário: {list}.",
            "Schedule updated. Now outside the new hours: {list}."
        );
    }
    Ok(a)
}

fn book(
    args: &Value,
    mut st: Value,
    actor: &Actor,
    now: i64,
    who: Role,
) -> Result<Applied, String> {
    if !st["configured"].as_bool().unwrap_or(false) {
        return Err(t(
            "a agenda ainda não está aberta",
            "the schedule isn't open yet",
        ));
    }
    let sid = s(args, "serviceId")
        .map(|x| service_id(&x))
        .ok_or_else(|| t("qual serviço?", "which service?"))?;
    let svc = st["services"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|x| x["id"].as_str() == Some(sid.as_str()))
        .cloned()
        .ok_or_else(|| t("serviço desconhecido", "unknown service"))?;
    let minutes = svc["minutes"].as_i64().unwrap_or(30);
    let start = args["startMs"]
        .as_i64()
        .ok_or_else(|| t("qual horário?", "which time?"))?;
    if let Some(end) = args.get("endMs").filter(|v| !v.is_null()) {
        if end.as_i64() != Some(start + minutes * MINUTE) {
            return Err(tr!(
                "{} dura {minutes} min",
                "{} takes {minutes} min",
                svc["name"].as_str().unwrap_or("")
            ));
        }
    }
    if who == Role::Client {
        let open = st["bookings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| is_for(b, actor.id) && occupies(b, now))
            .count();
        if open >= MAX_OPEN_PER_MEMBER {
            return Err(tr!(
                "você já tem {MAX_OPEN_PER_MEMBER} horários em aberto",
                "you already have {MAX_OPEN_PER_MEMBER} open bookings"
            ));
        }
    }
    let end = check_slot(&st, start, minutes, now, None)?;
    let id = next_id(&mut st, "b");
    let svc_name = svc["name"].as_str().unwrap_or_default().to_string();
    // Who the booking is for: the actor, or the client staff or the agent names.
    let (for_id, for_name) = match (who.staff(), s(args, "forName")) {
        (true, Some(n)) => (s(args, "forId").unwrap_or_default(), n),
        _ => (actor.id.to_string(), actor.name.to_string()),
    };
    st["bookings"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?
        .push(json!({
            "id": id,
            "serviceId": sid,
            "serviceName": svc_name,
            "minutes": minutes,
            "priceCents": svc["priceCents"],
            "startMs": start,
            "endMs": end,
            "byId": actor.id,
            "byName": clean(actor.name, 48),
            "forId": for_id,
            "forName": clean(&for_name, 48),
            "status": "pending",
            "note": note(args).unwrap_or_default(),
            "at": now
        }));
    // The agent's host stamps the message it was answering, so a rerun after a
    // crash finds this booking instead of asking for the slot again.
    if let Some(r) = args["ref"].as_str().filter(|_| who == Role::Agent) {
        if let Some(b) = st["bookings"].as_array_mut().and_then(|b| b.last_mut()) {
            b["ref"] = json!(clean(r, 64));
        }
    }
    push_event(&mut st, "book_request", &id, actor.id, now)?;
    push_log(
        &mut st,
        actor.name,
        &tr!("pediu {svc_name}", "requested {svc_name}"),
        now,
    );
    trim(&mut st, now);
    let when = when_label(&st, start);
    let a = Applied::new(
        st,
        tr!(
            "Novo pedido: {svc_name}, {when}",
            "New request: {svc_name}, {when}"
        ),
        tr!(
            "Pedido: {svc_name}, {when}. O agente confirma.",
            "Request: {svc_name}, {when}. The agent confirms."
        ),
    );
    // Staff hear of it from the agent's `booking` note to the owner, never in the
    // chat the request came from (clients may be reading it).
    Ok(a)
}

fn resolve(
    args: &Value,
    mut st: Value,
    actor: &Actor,
    now: i64,
    who: Role,
) -> Result<Applied, String> {
    if !who.staff() {
        return Err(t(
            "só o dono, o agente ou quem ele escolheu resolvem pedidos",
            "only the owner, the agent or a chosen resolver can resolve requests",
        ));
    }
    let idx = booking_index(&st, args)?;
    let action = s(args, "action").ok_or_else(|| {
        t(
            "confirmar, remarcar, cancelar ou manter?",
            "confirm, adjust, cancel or keep?",
        )
    })?;
    let b = st["bookings"][idx].clone();
    let id = b["id"].as_str().unwrap_or_default().to_string();
    if who == Role::Resolver && b["byId"].as_str() == Some(actor.id) {
        return Err(t(
            "quem pediu o horário não resolve o próprio pedido",
            "whoever requested the slot can't resolve their own request",
        ));
    }
    let status = b["status"].as_str().unwrap_or("pending").to_string();
    let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
    let start = b["startMs"].as_i64().unwrap_or(0);
    let what = match action.as_str() {
        "confirm" => resolve_confirm(&mut st, actor, now, idx, &b)?,

        "adjust" => resolve_adjust(&mut st, args, actor, now, idx, &b)?,
        "cancel" => {
            if !matches!(status.as_str(), "pending" | "confirmed") {
                return Err(t(
                    "esse horário já foi cancelado",
                    "that booking was cancelled",
                ));
            }
            st["bookings"][idx]["status"] = json!("cancelled");
            resolve_events(&mut st, &id, None, "cancelled", actor.id, now);
            tr!(
                "cancelou {svc_name}, {}",
                "cancelled {svc_name}, {}",
                when_label(&st, start)
            )
        }
        "keep" => {
            let cancel = has_open(&st, &id, "cancel_request");
            let hours = has_open(&st, &id, "outside_hours");
            if !cancel && !hours {
                return Err(t(
                    "não há nada aberto nesse horário",
                    "there's nothing open on that booking",
                ));
            }
            resolve_events(&mut st, &id, Some("cancel_request"), "kept", actor.id, now);
            if hours {
                resolve_events(&mut st, &id, Some("outside_hours"), "kept", actor.id, now);
                if let Some(m) = st["bookings"][idx].as_object_mut() {
                    m.remove("outsideHours");
                    // Kept on purpose: later hour changes don't flag it again.
                    m.insert("hoursKept".into(), json!(true));
                }
            }
            tr!(
                "manteve {svc_name}, {}",
                "kept {svc_name}, {}",
                when_label(&st, start)
            )
        }
        _ => return Err(t("ação desconhecida", "unknown action")),
    };
    if let Some(n) = note(args) {
        st["bookings"][idx]["note"] = json!(n);
    }
    push_log(&mut st, actor.name, &what, now);
    trim(&mut st, now);
    let mut a = Applied::new(st, format!("{} {what}", actor.name), format!("{what}."));
    a.comment = Some(format!("{what}."));
    Ok(a)
}

fn cancel(args: &Value, mut st: Value, actor: &Actor, now: i64) -> Result<Applied, String> {
    let idx = booking_index(&st, args)?;
    let b = st["bookings"][idx].clone();
    if !is_for(&b, actor.id) {
        return Err(t(
            "só quem marcou pode pedir o cancelamento",
            "only the person who booked can request a cancellation",
        ));
    }
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
    let a = match b["status"].as_str() {
        Some("pending") => {
            st["bookings"][idx]["status"] = json!("cancelled");
            resolve_events(
                &mut st,
                &id,
                Some("book_request"),
                "withdrawn",
                actor.id,
                now,
            );
            push_log(
                &mut st,
                actor.name,
                &tr!("desistiu de {svc_name}", "withdrew {svc_name}"),
                now,
            );
            trim(&mut st, now);
            Applied::new(
                st,
                tr!(
                    "Pedido retirado: {svc_name}",
                    "Request withdrawn: {svc_name}"
                ),
                t("Pedido cancelado.", "Request cancelled."),
            )
        }
        Some("confirmed") => {
            if has_open(&st, &id, "cancel_request") {
                return Err(t(
                    "o cancelamento já foi pedido",
                    "cancellation was already requested",
                ));
            }
            push_event(&mut st, "cancel_request", &id, actor.id, now)?;
            push_log(
                &mut st,
                actor.name,
                &tr!(
                    "pediu para cancelar {svc_name}",
                    "asked to cancel {svc_name}"
                ),
                now,
            );
            trim(&mut st, now);
            let when = when_label(&st, b["startMs"].as_i64().unwrap_or(0));
            Applied::new(
                st,
                tr!(
                    "Pedido de cancelamento: {svc_name}, {when}",
                    "Cancel request: {svc_name}, {when}"
                ),
                t("Pedido de cancelamento enviado.", "Cancel request sent."),
            )
        }
        _ => return Err(t("já está cancelado", "already cancelled")),
    };
    Ok(a)
}

fn resolve_confirm(
    st: &mut Value,
    actor: &Actor,
    now: i64,
    idx: usize,
    b: &Value,
) -> Result<String, String> {
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let status = b["status"].as_str().unwrap_or("pending").to_string();
    let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
    let start = b["startMs"].as_i64().unwrap_or(0);
    Ok({
        if status != "pending" {
            return Err(t(
                "esse pedido não está aguardando",
                "that request isn't waiting",
            ));
        }
        let clash = st["bookings"].as_array().into_iter().flatten().any(|o| {
            o["id"].as_str() != Some(id.as_str())
                && occupies(o, now)
                && o["startMs"].as_i64().unwrap_or(0) < b["endMs"].as_i64().unwrap_or(0)
                && o["endMs"].as_i64().unwrap_or(0) > start
        });
        if clash {
            return Err(t(
                "esse horário já está ocupado",
                "that time is already taken",
            ));
        }
        st["bookings"][idx]["status"] = json!("confirmed");
        resolve_events(st, &id, Some("book_request"), "confirmed", actor.id, now);
        tr!(
            "confirmou {svc_name}, {}",
            "confirmed {svc_name}, {}",
            when_label(st, start)
        )
    })
}

fn resolve_adjust(
    st: &mut Value,
    args: &Value,
    actor: &Actor,
    now: i64,
    idx: usize,
    b: &Value,
) -> Result<String, String> {
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let status = b["status"].as_str().unwrap_or("pending").to_string();
    let svc_name = b["serviceName"].as_str().unwrap_or_default().to_string();
    Ok({
        if !matches!(status.as_str(), "pending" | "confirmed") {
            return Err(t(
                "esse horário já foi cancelado",
                "that booking was cancelled",
            ));
        }
        let new_start = args["startMs"]
            .as_i64()
            .ok_or_else(|| t("para quando?", "to when?"))?;
        let minutes = b["minutes"].as_i64().unwrap_or(30);
        if let Some(end) = args.get("endMs").filter(|v| !v.is_null()) {
            if end.as_i64() != Some(new_start + minutes * MINUTE) {
                return Err(tr!(
                    "{svc_name} dura {minutes} min",
                    "{svc_name} takes {minutes} min"
                ));
            }
        }
        let end = check_slot(st, new_start, minutes, now, Some(&id))?;
        st["bookings"][idx]["startMs"] = json!(new_start);
        st["bookings"][idx]["endMs"] = json!(end);
        st["bookings"][idx]["status"] = json!("confirmed");
        if let Some(m) = st["bookings"][idx].as_object_mut() {
            m.remove("outsideHours");
        }
        // A new time answers everything open on it: the request, a cancel
        // request (the client gets the new time instead), an hours flag.
        resolve_events(st, &id, None, "adjusted", actor.id, now);
        tr!(
            "remarcou {svc_name} para {}",
            "moved {svc_name} to {}",
            when_label(st, new_start)
        )
    })
}
