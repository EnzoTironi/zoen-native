//! Existing app device grants, trust and state projection.
use super::*;

impl Engine {
    // ── MCP Apps: o núcleo é o servidor MCP local dos mini-apps embutidos ──

    // ── Native capabilities for mini-apps (photos, location, calendar…) ──
    //
    // One Grant per mini-app Item, from you to `app:<item>`, logged in that Space's event
    // log. "Allow once" = the same Grant expiring in a minute. Revoking appends GrantRevoked.

    pub(super) fn device_grants_for(&self, item: &str) -> impl Iterator<Item = &GrantState> {
        let grantee = format!("app:{item}");
        let me = self.me.as_deref();
        let now = now_ms();
        self.state.grants.iter().rev().filter(move |g| {
            !g.revoked
                && Some(g.grant.grantor.as_str()) == me
                && g.grant.grantee.as_deref() == Some(grantee.as_str())
                && g.grant.expires_at_ms.map(|e| e > now).unwrap_or(true)
                && matches!(g.grant.capability, Capability::Device { .. })
        })
    }

    pub fn app_device_allowed(&self, item: &str, capability: &str) -> bool {
        self.device_grants_for(item).any(|g| matches!(&g.grant.capability, Capability::Device { capability: c, .. } if c == capability))
    }

    pub fn grant_app_device(
        &mut self,
        item: &str,
        capability: &str,
        purpose: &str,
        always: bool,
    ) -> R<String> {
        let space = self
            .state
            .items
            .get(item)
            .map(|it| it.space.clone())
            .ok_or_else(|| not_found(&t("app", "app")))?;
        if !matches!(
            self.state.items.get(item).map(|it| &it.current().content),
            Some(ItemContent::App(_))
        ) {
            return Err(CoreError::Invalid {
                reason: t("não é um app", "not an app"),
            });
        }
        let cap = capability.trim();
        if cap.is_empty()
            || cap.len() > 64
            || !cap
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".:-_".contains(c))
        {
            return Err(CoreError::Invalid {
                reason: t("capacidade inválida", "invalid capability"),
            });
        }
        let me = self.me_id()?;
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(format!("app:{item}")),
            scope: GrantScope::Item(item.into()),
            capability: Capability::Device {
                capability: cap.into(),
                purpose: purpose.chars().take(160).collect(),
            },
            expires_at_ms: if always {
                None
            } else {
                Some(now_ms() + 60_000)
            },
        };
        let id = grant.id.clone();
        self.append(&space, &me, EventBody::GrantIssued { grant })?;
        Ok(id)
    }

    pub fn revoke_app_device(&mut self, grant_id: &str) -> R<()> {
        let g = self
            .state
            .grants
            .iter()
            .find(|g| g.grant.id == grant_id && !g.revoked)
            .ok_or_else(|| not_found(&t("concessão", "grant")))?;
        let GrantScope::Item(item) = &g.grant.scope else {
            return Err(CoreError::Invalid {
                reason: t("não é de um app", "not an app grant"),
            });
        };
        let space = self
            .state
            .items
            .get(item)
            .map(|it| it.space.clone())
            .ok_or_else(|| not_found(&t("app", "app")))?;
        let me = self.me_id()?;
        self.append(
            &space,
            &me,
            EventBody::GrantRevoked {
                grant: grant_id.into(),
            },
        )?;
        Ok(())
    }

    pub fn app_device_grants(&self) -> Vec<DeviceGrantDto> {
        let now = now_ms();
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.expires_at_ms.map(|e| e > now).unwrap_or(true))
            .filter_map(|g| {
                let Capability::Device {
                    capability,
                    purpose,
                } = &g.grant.capability
                else {
                    return None;
                };
                let GrantScope::Item(item) = &g.grant.scope else {
                    return None;
                };
                let it = self.state.items.get(item)?;
                let ItemContent::App(doc) = &it.current().content else {
                    return None;
                };
                Some(DeviceGrantDto {
                    grant_id: g.grant.id.clone(),
                    item_id: item.clone(),
                    item_title: doc.title.clone(),
                    app_name: apps::spec(&doc.app)
                        .map(|s| s.name.to_string())
                        .unwrap_or_else(|| doc.app.clone()),
                    space_title: self
                        .state
                        .spaces
                        .get(&it.space)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    capability: capability.clone(),
                    purpose: purpose.clone(),
                    always: g.grant.expires_at_ms.is_none(),
                    at_ms: g.at_ms,
                })
            })
            .collect()
    }

    pub(super) fn app_state_dto(&self, it: &ItemState) -> Option<AppStateDto> {
        let ItemContent::App(doc) = &it.current().content else {
            return None;
        };
        let state: Value = serde_json::from_str(&doc.state_json).unwrap_or(Value::Null);
        let now = now_ms();
        let (headline, metrics) = apps::headline(&doc.app, &state, now);
        Some(AppStateDto {
            app_id: doc.app.clone(),
            resource_uri: doc.resource_uri.clone(),
            name: apps::spec(&doc.app)
                .map(|s| s.name.to_string())
                .unwrap_or_default(),
            view_json: apps::view_for(&doc.app, &state, now, self.me.as_deref().unwrap_or(""))
                .to_string(),
            headline,
            metrics: metrics
                .into_iter()
                .map(|(label, value)| AppMetricDto { label, value })
                .collect(),
            last_action: apps::last_action(&state),
            trust: trust_dto(self.app_trust_in_space(&it.space, &it.id)),
            snapshot_json: apps::snapshot(&doc.app, &state, &it.id, now)
                .map(|v| v.to_string())
                .unwrap_or_default(),
        })
    }

    /// Nível da Concessão do mini-app neste Item (sem Concessão: só ouvir).
    pub(crate) fn app_trust(&self, item: &str) -> TrustLevel {
        self.state.items.get(item).map_or(TrustLevel::Listen, |it| {
            self.app_trust_in_space(&it.space, item)
        })
    }

    pub(super) fn app_trust_in_space(&self, space: &str, item: &str) -> TrustLevel {
        let grantee = format!("app:{item}");
        self.space_projections
            .get(space)
            .into_iter()
            .flat_map(|scoped| scoped.grants.iter())
            .rev()
            .filter(|g| {
                !g.revoked
                    && g.grant.grantee.as_deref() == Some(grantee.as_str())
                    && g.grant.scope == GrantScope::Item(item.to_string())
                    && g.grant.expires_at_ms.is_none_or(|at| at > now_ms())
            })
            .find_map(|g| match g.grant.capability {
                Capability::Trust(l) => Some(l),
                _ => None,
            })
            .unwrap_or(TrustLevel::Listen)
    }
}
