//! Non-model options worker, using the same confirmed encrypted control channel.
use super::*;
pub struct OwnerPolicyInquiryControl {
    source: String,
    sequence: u64,
    signed: SignedOwnerPolicyInquiry,
    digest: String,
}
impl OwnerPolicyInquiryControl {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn space(&self) -> &str {
        &self.signed.inquiry.space
    }
    pub fn agent(&self) -> &str {
        &self.signed.inquiry.agent
    }
    pub fn signed(&self) -> &SignedOwnerPolicyInquiry {
        &self.signed
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
impl Engine {
    pub(crate) fn owner_policy_inquiry(
        &self,
        space: &str,
        source: &str,
    ) -> R<OwnerPolicyInquiryControl> {
        let agent = self.owner_policy_agent(space)?;
        let owner = self
            .identities
            .get(&agent)
            .and_then(|profile| profile.owner.as_deref())
            .ok_or_else(denied)?;
        let event = self
            .logs
            .get(space)
            .and_then(|log| log.events().iter().find(|event| event.client_id == source))
            .ok_or_else(denied)?;
        let EventBody::OwnerPolicyOptionsRequested {
            agent: target,
            signed,
        } = &event.body
        else {
            return Err(denied());
        };
        if target != &agent
            || signed.inquiry.agent != agent
            || signed.inquiry.owner != owner
            || signed.inquiry.space != space
            || event.author != owner
            || event.seq == 0
            || event.sealed_wire.is_none()
            || event.device.as_deref() != Some(signed.device.as_str())
            || event.cert.as_deref() != Some(signed.cert.as_str())
            || event_log::verify_author(event).is_err()
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(denied());
        }
        self.owner_policy_image_input(&signed.inquiry, crate::engine::now_ms())?;
        Ok(OwnerPolicyInquiryControl {
            source: source.into(),
            sequence: event.seq,
            signed: signed.clone(),
            digest: event_log::owner_policy_inquiry::digest(&signed.inquiry).ok_or_else(invalid)?,
        })
    }
    fn owner_policy_options_reported(&self, query: &OwnerPolicyInquiryControl) -> R<bool> {
        let pending = self.store.outbox()?;
        Ok(pending
            .iter()
            .map(|entry| &entry.event)
            .chain(
                self.logs
                    .get(query.space())
                    .into_iter()
                    .flat_map(|log| log.events()),
            )
            .any(|event| {
                let matches = match &event.body {
                    EventBody::OwnerPolicyOptionsReported { source, options } => {
                        source == query.source()
                            && options.inquiry_digest == query.digest()
                            && validate_options(query.signed(), options, crate::engine::now_ms())
                                .is_ok()
                    }
                    EventBody::OwnerPolicyOptionsRefused {
                        source,
                        inquiry_digest,
                    } => source == query.source() && inquiry_digest == query.digest(),
                    _ => false,
                };
                matches
                    && event.space == query.space()
                    && event.author == query.agent()
                    && event.device.is_some()
                    && event.cert.is_some()
                    && event_log::verify_author(event).is_ok()
            }))
    }
    pub(crate) fn refuse_owner_policy_options(
        &mut self,
        query: &OwnerPolicyInquiryControl,
    ) -> R<()> {
        let current = self.owner_policy_inquiry(query.space(), query.source())?;
        if current.digest() != query.digest() {
            return Err(denied());
        }
        if self.owner_policy_options_reported(&current)? {
            return Ok(());
        }
        let author = self
            .net
            .account_author(current.agent())
            .ok_or_else(denied)?;
        self.append_owner_policy_report(
            current.space(),
            &author,
            EventBody::OwnerPolicyOptionsRefused {
                source: current.source().into(),
                inquiry_digest: current.digest().into(),
            },
        )?;
        Ok(())
    }
    pub(crate) fn report_owner_policy_options(
        &mut self,
        query: &OwnerPolicyInquiryControl,
        options: OwnerPolicyOptions,
    ) -> R<()> {
        let current = self.owner_policy_inquiry(query.space(), query.source())?;
        if current.digest() != query.digest() {
            return Err(denied());
        }
        validate_options(current.signed(), &options, crate::engine::now_ms())?;
        if self.owner_policy_options_reported(&current)? {
            return Ok(());
        }
        let author = self
            .net
            .account_author(current.agent())
            .ok_or_else(denied)?;
        self.append_owner_policy_report(
            current.space(),
            &author,
            EventBody::OwnerPolicyOptionsReported {
                source: current.source().into(),
                options,
            },
        )?;
        Ok(())
    }
}
impl Client {
    pub fn owner_policy_inquiry(&self, space: &str, source: &str) -> R<OwnerPolicyInquiryControl> {
        self.lock().owner_policy_inquiry(space, source)
    }
    pub fn owner_policy_inquiries(
        &self,
        space: &str,
        after_seq: u64,
        limit: u32,
    ) -> R<Vec<OwnerPolicyInquiryControl>> {
        if !(1..=50).contains(&limit) {
            return Err(invalid());
        }
        let engine = self.lock();
        engine.owner_policy_agent(space)?;
        let mut queries = Vec::new();
        for event in engine.logs.get(space).ok_or_else(denied)?.events() {
            if event.seq <= after_seq
                || !matches!(event.body, EventBody::OwnerPolicyOptionsRequested { .. })
            {
                continue;
            }
            match engine.owner_policy_inquiry(space, &event.client_id) {
                Ok(query) => {
                    if !engine.owner_policy_options_reported(&query)? {
                        queries.push(query);
                    }
                }
                Err(CoreError::Forbidden { .. }) => continue,
                Err(error) => return Err(error),
            }
            if queries.len() == limit as usize {
                break;
            }
        }
        Ok(queries)
    }
    /// Explicit verified Runtime refusal only. Ambiguous/unavailable stays pending.
    pub fn refuse_owner_policy_options(&self, query: &OwnerPolicyInquiryControl) -> R<()> {
        self.lock().refuse_owner_policy_options(query)
    }
    /// Call only with a verified actual Runtime options reply; never model data.
    pub fn report_owner_policy_options(
        &self,
        query: &OwnerPolicyInquiryControl,
        options: OwnerPolicyOptions,
    ) -> R<()> {
        self.lock().report_owner_policy_options(query, options)
    }
}
