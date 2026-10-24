//! Genuine Runtime options, correlated with one authenticated owner inquiry.
mod account;
mod control;
mod index;
mod prepare;
use super::*;
pub use control::{OwnerPolicyInquiryControl, OwnerPolicyInquiryCounts, OwnerPolicyInquiryScan};
use types::owner_policy_context::{
    OwnerPolicyInquiry, OwnerPolicyOptions, SignedOwnerPolicyInquiry, CONTEXT_TTL_MS,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct OwnerPolicyContext {
    pub options_digest: String,
    pub currency: String,
    pub scale: u8,
    pub year: i32,
    pub month: u8,
    pub reservation_units: String,
    pub current_limit_units: Option<String>,
    pub current_max_attempt_units: Option<String>,
    pub current_enabled: Option<bool>,
    pub current_expires_at_ms: Option<String>,
    pub current_expired: Option<bool>,
    pub current_policy_digest: Option<String>,
    pub existing_profile_count: u32,
    pub image_profile_digest: String,
    pub image_size: String,
    pub image_quality: String,
    pub observed_at_ms: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct OwnerPolicyOptionsStatus {
    pub source: String,
    pub inquiry_digest: String,
    pub state: String,
    pub context: Option<OwnerPolicyContext>,
}
fn options_digest(options: &OwnerPolicyOptions) -> R<String> {
    Ok(event_log::sha256_hex(
        &serde_json::to_vec(&("zoen-owner-policy-options/1", options)).map_err(|_| invalid())?,
    ))
}
pub(super) fn validate_options(
    signed: &SignedOwnerPolicyInquiry,
    options: &OwnerPolicyOptions,
    at_ms: i64,
) -> R<()> {
    let inquiry = &signed.inquiry;
    if !event_log::owner_policy_inquiry::verify(signed)
        || !inquiry.fresh_at(at_ms)
        || !options.valid_shape()
        || event_log::owner_policy_inquiry::digest(inquiry).as_deref()
            != Some(options.inquiry_digest.as_str())
        || (inquiry.purpose.is_none() && inquiry.price_digest != options.image_profile_digest)
        || (inquiry.purpose.is_some() && options.image_spec.is_none())
        || options.observed_at_ms < inquiry.at_ms.saturating_sub(30_000)
        || options.observed_at_ms > at_ms.saturating_add(30_000)
        || at_ms.saturating_sub(options.observed_at_ms) > CONTEXT_TTL_MS
    {
        return Err(denied());
    }
    use chrono::Datelike;
    let date =
        chrono::DateTime::from_timestamp_millis(options.observed_at_ms).ok_or_else(invalid)?;
    if date.year() != options.year || date.month() != u32::from(options.month) {
        return Err(denied());
    }
    if let Some(current) = &options.current_policy {
        if !event_log::owner_budget::verify(current)
            || current.policy.owner != inquiry.owner
            || current.policy.year != options.year
            || current.policy.month != options.month
            || current.policy.currency != options.currency
            || current.policy.scale != options.scale
            || event_log::owner_budget::digest(&current.policy) != options.current_policy_digest
        {
            return Err(denied());
        }
    }
    Ok(())
}

impl Engine {
    fn owner_policy_query(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
    ) -> R<SignedOwnerPolicyInquiry> {
        let signed = self.owner_policy_query_record(scope, source)?;
        self.owner_policy_context_input(&signed.inquiry, crate::engine::now_ms())?;
        Ok(signed)
    }

    fn owner_policy_query_record(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
    ) -> R<SignedOwnerPolicyInquiry> {
        self.owner_policy_conversation(scope)?;
        let pending = self.store.outbox_get(source)?;
        let event = self
            .logs
            .get(&scope.space_id)
            .and_then(|log| log.events().iter().find(|event| event.client_id == source))
            .or(pending.as_ref())
            .ok_or_else(denied)?;
        let EventBody::OwnerPolicyOptionsRequested { agent, signed } = &event.body else {
            return Err(denied());
        };
        if event.space != scope.space_id
            || event.author != scope.expected_identity_id
            || agent != &scope.agent_id
            || signed.inquiry.owner != event.author
            || signed.inquiry.agent != *agent
            || signed.inquiry.space != scope.space_id
            || event.device.as_deref() != Some(signed.device.as_str())
            || event.cert.as_deref() != Some(signed.cert.as_str())
            || event_log::verify_author(event).is_err()
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(denied());
        }
        Ok(signed.clone())
    }
    pub(crate) fn find_owner_policy_options(
        &self,
        scope: &OwnerPolicyScope,
        request: &str,
        expected_hash: &str,
    ) -> R<Option<OwnerPolicyOptionsStatus>> {
        self.find_policy_inquiry(scope, None, request, expected_hash)
    }

    pub(crate) fn request_owner_policy_options(
        &mut self,
        scope: &OwnerPolicyScope,
        request: &str,
        expected_hash: &str,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.owner_policy_conversation(scope)?;
        let record = self.state.requests.get(request).ok_or_else(denied)?;
        let review = record
            .req
            .tool_proposal
            .as_ref()
            .and_then(|proposal| self.image_generation_review(proposal))
            .ok_or_else(denied)?;
        let inquiry = OwnerPolicyInquiry {
            id: types::new_id("policy_inquiry"),
            owner: scope.expected_identity_id.clone(),
            agent: scope.agent_id.clone(),
            space: scope.space_id.clone(),
            request: request.into(),
            content_hash: expected_hash.into(),
            price_digest: review.price_digest,
            purpose: None,
            at_ms: crate::engine::now_ms(),
        };
        self.owner_policy_image_input(&inquiry, inquiry.at_ms)?;
        self.queue_owner_policy_inquiry(scope, inquiry)
    }
    pub(crate) fn queue_owner_policy_inquiry(
        &mut self,
        scope: &OwnerPolicyScope,
        inquiry: OwnerPolicyInquiry,
    ) -> R<OwnerPolicyOptionsStatus> {
        // Management may queue while this explicitly selected private DM is
        // establishing MLS. The must-seal outbox never offers a clear fallback;
        // actual consumption, review and submission still require ready MLS.
        if inquiry.purpose.is_some() {
            self.owner_policy_private_conversation(scope, false)?;
            if !inquiry.fresh_at(crate::engine::now_ms()) {
                return Err(denied());
            }
        } else {
            self.owner_policy_conversation(scope)?;
            self.owner_policy_context_input(&inquiry, inquiry.at_ms)?;
        }
        let author = self
            .net
            .account_author(&scope.expected_identity_id)
            .ok_or_else(denied)?;
        let signed = event_log::owner_policy_inquiry::sign(&author, inquiry).ok_or_else(denied)?;
        let digest =
            event_log::owner_policy_inquiry::digest(&signed.inquiry).ok_or_else(invalid)?;
        self.atomic_agent_action(|engine| {
            let event = engine.append_synced(
                &scope.space_id,
                &author,
                crate::engine::now_ms(),
                EventBody::OwnerPolicyOptionsRequested {
                    agent: scope.agent_id.clone(),
                    signed,
                },
            )?;
            engine.retain_owner_policy_inquiry(scope, &event)?;
            Ok(OwnerPolicyOptionsStatus {
                source: event.client_id,
                inquiry_digest: digest,
                state: "pending".into(),
                context: None,
            })
        })
    }
    fn owner_policy_options_record(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
    ) -> R<(SignedOwnerPolicyInquiry, Option<OwnerPolicyOptions>)> {
        let inquiry = self.owner_policy_query(scope, source)?;
        let log = self.logs.get(&scope.space_id).ok_or_else(denied)?;
        let original = log.events().iter().find(|event| event.client_id == source);
        let Some(original) = original else {
            return Ok((inquiry, None));
        };
        let options = log.events().iter().rev().find_map(|event| {
            let EventBody::OwnerPolicyOptionsReported {
                source: target,
                options,
            } = &event.body
            else {
                return None;
            };
            if target != source
                || event.author != scope.agent_id
                || event.seq <= original.seq
                || event.sealed_wire.is_none()
                || event.device.is_none()
                || event.cert.is_none()
                || event_log::verify_author(event).is_err()
                || validate_options(&inquiry, options, crate::engine::now_ms()).is_err()
            {
                return None;
            }
            Some(options.clone())
        });
        Ok((inquiry, options))
    }
    pub(crate) fn owner_policy_options(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.owner_policy_options_at(scope, source, crate::engine::now_ms())
    }

    /// Project terminal expiry using one observed clock; execution keeps its
    /// independent strict freshness checks.
    pub(crate) fn owner_policy_options_at(
        &self,
        scope: &OwnerPolicyScope,
        source: &str,
        at_ms: i64,
    ) -> R<OwnerPolicyOptionsStatus> {
        let original = self.owner_policy_query_record(scope, source)?;
        if self.owner_policy_inquiry_expired(&original.inquiry, at_ms)? {
            return Ok(OwnerPolicyOptionsStatus {
                source: source.into(),
                inquiry_digest: event_log::owner_policy_inquiry::digest(&original.inquiry)
                    .ok_or_else(invalid)?,
                state: "expired".into(),
                context: None,
            });
        }
        let (inquiry, options) = self.owner_policy_options_record(scope, source)?;
        let input = self.owner_policy_context_input(&inquiry.inquiry, crate::engine::now_ms())?;
        let digest =
            event_log::owner_policy_inquiry::digest(&inquiry.inquiry).ok_or_else(invalid)?;
        let context = options
            .as_ref()
            .map(|options| -> R<OwnerPolicyContext> {
                let current = options.current_policy.as_ref().map(|signed| &signed.policy);
                Ok(OwnerPolicyContext {
                    options_digest: options_digest(options)?,
                    currency: options.currency.clone(),
                    scale: options.scale,
                    year: options.year,
                    month: options.month,
                    reservation_units: options.reservation_units.to_string(),
                    current_limit_units: current.map(|policy| policy.limit_units.to_string()),
                    current_max_attempt_units: current
                        .map(|policy| policy.max_attempt_units.to_string()),
                    current_enabled: current.map(|policy| policy.enabled),
                    current_expires_at_ms: current.map(|policy| policy.expires_at_ms.to_string()),
                    current_expired: current
                        .map(|policy| policy.expires_at_ms <= options.observed_at_ms),
                    current_policy_digest: options.current_policy_digest.clone(),
                    existing_profile_count: u32::try_from(
                        current.map_or(0, |policy| policy.allowed_profiles.len()),
                    )
                    .map_err(|_| invalid())?,
                    image_profile_digest: options.image_profile_digest.clone(),
                    image_size: match &input {
                        Some(input) => input.definition.size.clone(),
                        None => options.image_spec.as_ref().ok_or_else(denied)?.size.clone(),
                    },
                    image_quality: match &input {
                        Some(input) => input.definition.quality.clone(),
                        None => options
                            .image_spec
                            .as_ref()
                            .ok_or_else(denied)?
                            .quality
                            .clone(),
                    },
                    observed_at_ms: options.observed_at_ms.to_string(),
                })
            })
            .transpose()?;
        let refused = self.logs.get(&scope.space_id).into_iter().flat_map(|log| log.events()).any(|event| {
            matches!(&event.body, EventBody::OwnerPolicyOptionsRefused { source: target, inquiry_digest }
                if target == source && inquiry_digest == &digest && event.author == scope.agent_id
                && event.seq > 0 && event.sealed_wire.is_some() && event.device.is_some() && event.cert.is_some()
                && event_log::verify_author(event).is_ok())
        });
        let failed = self.store.outbox()?.iter().any(|entry| {
            entry.failed && entry.event.client_id == source && entry.event.space == scope.space_id
        });
        Ok(OwnerPolicyOptionsStatus {
            source: source.into(),
            inquiry_digest: digest,
            state: if context.is_some() {
                "ready"
            } else if refused {
                "refused"
            } else if failed {
                "delivery_failed"
            } else {
                "pending"
            }
            .into(),
            context,
        })
    }
}
impl Client {
    /// Read-only recovery for a lost inquiry acknowledgement.
    pub fn find_owner_policy_options(
        &self,
        scope: OwnerPolicyScope,
        request_id: String,
        expected_hash: String,
    ) -> R<Option<OwnerPolicyOptionsStatus>> {
        self.lock()
            .find_owner_policy_options(&scope, &request_id, &expected_hash)
    }
    pub fn request_owner_policy_options(
        &self,
        scope: OwnerPolicyScope,
        request: String,
        expected_hash: String,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.lock()
            .request_owner_policy_options(&scope, &request, &expected_hash)
    }
    pub fn owner_policy_options(
        &self,
        scope: OwnerPolicyScope,
        source: String,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.lock().owner_policy_options(&scope, &source)
    }
}
