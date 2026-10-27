//! Implementation within the existing browser client authority.
use super::*;

impl<S: Signer> Client<S> {
    /// Disposable virtual ItemCreated at each authenticated proposal receipt position.
    /// Its content bytes remain the original receipt; this is never a signed event.
    pub(in crate::client) fn approved_request_items(
        &self,
    ) -> Result<BTreeMap<(String, String), Event>, String> {
        Ok(self.agent_projection()?.virtual_items)
    }

    /// Already-authorized, live Item-scoped grants. The mini-app host uses this
    /// projection too; it must not reconstruct a second permission tape.
    pub(in crate::client) fn active_app_grants(
        &self,
        item: &str,
        now: i64,
    ) -> Result<Vec<Grant>, String> {
        let state = self.agent_projection()?;
        if !state
            .items
            .get(item)
            .is_some_and(|item| matches!(item.current().content, ItemContent::App(_)))
        {
            return Err("App indisponível.".into());
        }
        let subject = format!("app:{item}");
        Ok(state
            .grants
            .into_iter()
            .filter(|grant| {
                !grant.revoked
                    && grant.grant.expires_at_ms.is_none_or(|at| at > now)
                    && grant.grant.grantee.as_deref() == Some(subject.as_str())
                    && grant.grant.scope == GrantScope::Item(item.into())
            })
            .map(|grant| grant.grant)
            .collect())
    }

    pub(in crate::client) fn grant_browser_app_device(
        &mut self,
        item: &str,
        capability: &str,
        purpose: &str,
        always: bool,
        now: i64,
    ) -> Result<String, String> {
        let state = self.agent_projection()?;
        let app = state
            .items
            .get(item)
            .filter(|item| matches!(item.current().content, ItemContent::App(_)))
            .ok_or("App indisponível.")?;
        let capability = capability.trim();
        if capability.is_empty()
            || capability.len() > 64
            || !capability
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || ".:-_".contains(value))
        {
            return Err("Capacidade inválida.".into());
        }
        let id = format!(
            "gr_{}",
            types::new_ulid(now).map_err(|error| error.to_string())?
        );
        let grant = Grant {
            id: id.clone(),
            grantor: self.account.identity.clone(),
            grantee: Some(format!("app:{item}")),
            scope: GrantScope::Item(item.into()),
            capability: Capability::Device {
                capability: capability.into(),
                purpose: purpose.chars().take(160).collect(),
            },
            expires_at_ms: if always {
                None
            } else {
                Some(now.checked_add(60_000).ok_or("Data fora do limite.")?)
            },
        };
        self.queue_domain_body(
            &app.space,
            EventBody::GrantIssued { grant },
            now,
            Vec::new(),
        )?;
        Ok(id)
    }

    pub(super) fn queue_agent_bodies(
        &mut self,
        bodies: Vec<(String, EventBody)>,
        now: i64,
    ) -> Result<(), String> {
        let outbox = self.saved.outbox.clone();
        let storage = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .clone();
        for (space, body) in bodies {
            if let Err(error) = self.queue_domain_body(&space, body, now, Vec::new()) {
                self.saved.outbox = outbox;
                *self
                    .provider
                    .storage()
                    .values
                    .write()
                    .map_err(|_| "MLS storage lock failed")? = storage;
                return Err(error);
            }
        }
        Ok(())
    }
    pub(super) fn decide_browser_request(
        &mut self,
        id: &str,
        decision: RequestDecision,
        now: i64,
    ) -> Result<Value, String> {
        let state = self.agent_projection()?;
        let original = state
            .requests
            .get(id)
            .ok_or("Este pedido não está disponível neste aparelho.")?;
        let approve = matches!(
            decision,
            RequestDecision::Approve | RequestDecision::AlwaysApprove
        );
        let standing = matches!(
            decision,
            RequestDecision::AlwaysApprove | RequestDecision::AlwaysDeny
        );
        if self.agent_owner(&original.request.agent) != Some(self.account.identity.as_str())
            || !writer(&state, &original.space, &self.account.identity)
        {
            return Err("Só o dono do agente com permissão nesta conversa pode decidir.".into());
        }
        if standing
            && approve
            && !standing_allow_permitted(&original.request.action, &Policy::default())
        {
            return Err("Esta ação sempre exige uma decisão individual.".into());
        }
        if let Some(previous) = original.approved {
            let receipt = original
                .receipt
                .as_ref()
                .ok_or("Este pedido já foi decidido.")?;
            let same_standing = receipt.standing_grant.as_ref().is_some_and(|given| {
                self.agent_standing(
                    &state,
                    &original.request.agent,
                    &original.space,
                    &original.request.action,
                    now,
                )
                .is_some_and(|(current, allow)| current.grant.id == given.id && allow == approve)
            });
            if previous != approve || standing && !same_standing {
                return Err(
                    "Este pedido já recebeu outra decisão, ou sua permissão permanente mudou."
                        .into(),
                );
            }
            return Ok(
                json!({"request":self.agent_request_value(&state, original, now)?,"message":receipt.message,"standing_grant_id":receipt.standing_grant.as_ref().map(|grant|&grant.id),"also_resolved":0}),
            );
        }
        let mut covered = vec![original.clone()];
        if standing {
            covered.extend(
                state
                    .requests
                    .values()
                    .filter(|other| {
                        other.request.id != id
                            && other.approved.is_none()
                            && other.space == original.space
                            && other.request.agent == original.request.agent
                            && standing_key(&other.request.action)
                                == standing_key(&original.request.action)
                    })
                    .cloned(),
            );
        }
        let mut bodies = Vec::new();
        let mut also = 0;
        let mut message = String::new();
        let mut claimed_items: BTreeSet<_> = state.items.keys().cloned().collect();
        for read in covered {
            if approve {
                let current = self.agent_request_current(&state, &read.request, &read.space)?;
                if current != RequestScope::Current {
                    if read.request.id != id {
                        continue;
                    }
                    return Err(if current == RequestScope::Waiting {
                        "O conteúdo confirmado ainda está chegando."
                    } else {
                        "O conteúdo ou a autorização mudou. Peça uma nova proposta."
                    }
                    .into());
                }
                if !standing_allow_permitted(&read.request.action, &Policy::default()) && standing {
                    continue;
                }
                if let Some(proposal) = &read.request.proposal {
                    if !claimed_items.insert(proposal.item.clone()) {
                        continue;
                    }
                }
                if let Some(item) = &read.request.item {
                    if state
                        .items
                        .get(item)
                        .is_some_and(|item| item.kind == ItemKind::Plan)
                    {
                        let read_plan = self.document_request(
                            &json!({"op":"plan_page","item_id":item}).to_string(),
                            now,
                        )?;
                        if read_plan.result["legacy_edit_admitted"] != true {
                            return Err(
                                "Este plano não admite esta operação; peça uma nova proposta."
                                    .into(),
                            );
                        }
                    }
                }
            }
            let text = if approve {
                read.request
                    .proposal
                    .as_ref()
                    .map(|proposal| proposal.message.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "Aprovado: {}. Aguardando a execução da ferramenta.",
                            read.request.title
                        )
                    })
            } else {
                format!("Ok, não vou {}.", read.request.title)
            };
            let receipt = RequestResolution {
                tool_decision: None,
                message: text.clone(),
                created_item: if approve {
                    read.request
                        .proposal
                        .as_ref()
                        .map(|proposal| proposal.item.clone())
                } else {
                    None
                },
                ai_cost_cents: read
                    .request
                    .proposal
                    .as_ref()
                    .map_or(0, |proposal| proposal.ai_cost_cents),
                standing_grant: (standing && read.request.id == id).then(|| Grant {
                    id: format!("gr_standing_{}", read.request.id),
                    grantor: self.account.identity.clone(),
                    grantee: Some(read.request.agent.clone()),
                    scope: GrantScope::Space(read.space.clone()),
                    capability: Capability::Standing {
                        action: standing_key(&read.request.action).into(),
                        allow: approve,
                    },
                    expires_at_ms: None,
                }),
            };
            if read.request.id == id {
                message = text;
            } else {
                also += 1;
            }
            bodies.push((
                read.space,
                EventBody::RequestResolved {
                    request: read.request.id,
                    approved: approve,
                    content_hash: read.request.content_hash,
                    resolution: Some(receipt),
                },
            ));
        }
        self.queue_agent_bodies(bodies, now)?;
        let next = self.agent_projection()?;
        let read = next
            .requests
            .get(id)
            .ok_or("Não foi possível recuperar a decisão.")?;
        Ok(
            json!({"request": self.agent_request_value(&next, read, now)?, "message": message, "standing_grant_id": read.receipt.as_ref().and_then(|receipt| receipt.standing_grant.as_ref()).map(|grant| &grant.id), "also_resolved":also}),
        )
    }
}
