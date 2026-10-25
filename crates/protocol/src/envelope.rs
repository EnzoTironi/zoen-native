//! Exact signed envelope and sequenced-log admission.
use super::{
    content_hash, decode_body, event_from_content, signed_content_hash, stub_content,
    verify_author, verify_sig, Author, Event, EventBody, LogError, Payload, Sealed, SealedKind,
    Seen, SignedContent,
};

/// What a device publishes: the author's signed bytes, not sequenced yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub(super) content: Vec<u8>,
    pub(super) parsed: SignedContent,
    pub sig: String,
    pub cert: Option<String>,
    /// Transport only, never stored: the invite code that lets a newcomer add themselves.
    pub invite: Option<String>,
    /// Already-pruned v3 entries retain the original hash carried by old relays.
    /// Its signature authenticates that hash, not the surviving header. Those lost
    /// bytes cannot be recovered by an upgrade; new v4 stubs authenticate both.
    legacy_pruned: Option<String>,
}

impl Envelope {
    /// `None` when `content` isn't a v3 signed content.
    #[must_use]
    pub fn new(
        content: Vec<u8>,
        sig: String,
        cert: Option<String>,
        invite: Option<String>,
    ) -> Option<Self> {
        if invite
            .as_deref()
            .is_some_and(|code| !valid_invite_code(code))
        {
            return None;
        }
        let parsed = SignedContent::parse(&content)?;
        parsed.payload.as_ref()?;
        Some(Self {
            content,
            parsed,
            sig,
            cert,
            invite,
            legacy_pruned: None,
        })
    }

    /// The stub a pruned sealed entry leaves (ADR 0026): the same header and kind and the
    /// hash of the MLS bytes, without them. It hashes and verifies like the original.
    /// `None` for anything that isn't sealed, or is already a stub.
    #[must_use]
    pub fn pruned(&self) -> Option<Envelope> {
        if self.is_pruned()
            || (self.parsed.v == 3 && self.wire_hash() == content_hash(&self.content))
        {
            // Full legacy ciphertext remains intact: its old signature cannot
            // authenticate a replacement digest. Never create another weak stub.
            return None;
        }
        let content = stub_content(&self.content)?;
        Envelope::new(content, self.sig.clone(), self.cert.clone(), None)
    }

    /// A stub the relay left when it pruned this entry.
    #[must_use]
    pub fn is_pruned(&self) -> bool {
        self.legacy_pruned.is_some()
            || matches!(&self.parsed.payload, Some(Payload::Sealed(s)) if s.is_stub())
    }

    #[must_use]
    pub fn legacy_pruned_hash(&self) -> Option<&str> {
        self.legacy_pruned.as_deref()
    }

    pub(crate) fn with_legacy_pruned(mut self, hash: Option<String>) -> Option<Self> {
        if let Some(original) = &hash {
            if self.parsed.v != 3
                || !matches!(&self.parsed.payload, Some(Payload::Sealed(s)) if s.data.is_empty() && s.data_hash.is_empty())
                || original.len() != 64
                || !original.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
        }
        self.legacy_pruned = hash;
        Some(self)
    }

    /// Wraps a signed, unsequenced plaintext event.
    ///
    /// # Errors
    /// Rejects malformed signed content rather than publishing an unreadable envelope.
    pub fn plain(e: &Event) -> Result<Self, LogError> {
        Self::new(e.content.clone(), e.sig.clone(), e.cert.clone(), None)
            .ok_or(LogError::BadContent { seq: e.seq })
    }

    /// Wraps MLS ciphertext signed by `author`'s device.
    ///
    /// # Errors
    /// Rejects signed content that cannot be decoded with the current content contract.
    pub fn sealed(
        author: &Author,
        space: &str,
        client_id: &str,
        at_ms: i64,
        seen: Option<&Seen>,
        sealed: Sealed,
    ) -> Result<Self, LogError> {
        let (content, sig) = author.sign_sealed(space, client_id, at_ms, seen, sealed);
        Self::new(content, sig, author.cert.clone(), None).ok_or(LogError::BadContent { seq: 0 })
    }

    /// The MLS bytes and their kind, for envelopes MLS opens.
    #[must_use]
    pub fn sealed_data(&self) -> Option<(SealedKind, &[u8])> {
        match &self.parsed.payload {
            Some(Payload::Sealed(s)) => Some((
                SealedKind::try_from(s.kind).unwrap_or(SealedKind::Unspecified),
                &s.data,
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn recovery(&self) -> Option<&[u8]> {
        // Legacy tag-5/v3 stubs authenticate their retained digest, not these
        // surviving header fields. Recovery references were introduced in v4.
        if self.parsed.v != 4 || self.legacy_pruned.is_some() {
            return None;
        }
        match &self.parsed.payload {
            Some(Payload::Sealed(s)) if !s.recovery.is_empty() => Some(&s.recovery),
            _ => None,
        }
    }

    #[must_use]
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    #[must_use]
    pub fn space(&self) -> &str {
        &self.parsed.space
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.parsed.client_id
    }

    #[must_use]
    pub fn author(&self) -> &str {
        &self.parsed.author
    }

    #[must_use]
    pub fn device(&self) -> Option<&str> {
        self.parsed.device.as_deref()
    }

    #[must_use]
    pub fn at_ms(&self) -> i64 {
        self.parsed.at_ms
    }

    #[must_use]
    pub fn seen(&self) -> Option<Seen> {
        self.parsed.seen()
    }

    #[must_use]
    pub fn is_sealed(&self) -> bool {
        matches!(self.parsed.payload, Some(Payload::Sealed(_)))
    }

    /// What a sealed envelope carries, from its clear framing (`None` when plain).
    #[must_use]
    pub fn sealed_kind(&self) -> Option<SealedKind> {
        self.sealed_data().map(|(kind, _)| kind)
    }

    /// The body, for envelopes the relay may read (`None` when sealed).
    #[must_use]
    pub fn body(&self) -> Option<EventBody> {
        match &self.parsed.payload {
            Some(Payload::Body(b)) => Some(decode_body(b)),
            _ => None,
        }
    }

    /// The hash the chain links: over the exact bytes, plain or sealed.
    #[must_use]
    pub fn wire_hash(&self) -> String {
        self.legacy_pruned
            .clone()
            .unwrap_or_else(|| signed_content_hash(&self.content, &self.sig))
    }

    /// Checks the device certificate and the author's signature over the bytes.
    ///
    /// # Errors
    /// Rejects an invalid device certificate, signer key or content signature.
    pub fn verify(&self) -> Result<(), LogError> {
        if let Some(e) = self.plain_event() {
            return verify_author(&e);
        }
        let signer = match self.device() {
            Some(d) => {
                let cert = self
                    .cert
                    .as_deref()
                    .ok_or(LogError::BadDeviceCert { seq: 0 })?;
                if !verify_sig(self.author(), &event_log::device_cert_message(d), cert) {
                    return Err(LogError::BadDeviceCert { seq: 0 });
                }
                d
            }
            None => self.author(),
        };
        if verify_sig(
            signer,
            &event_log::content::signature_message(&self.content, &self.wire_hash()),
            &self.sig,
        ) {
            Ok(())
        } else {
            Err(LogError::BadSignature { seq: 0 })
        }
    }

    /// The event as the relay sees it (plain only), unsequenced.
    #[must_use]
    pub fn plain_event(&self) -> Option<Event> {
        if self.is_sealed() {
            return None;
        }
        event_from_content(
            self.content.clone(),
            self.sig.clone(),
            self.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .ok()
    }

    /// Size guard input: the bytes the relay will store.
    pub fn stored_len(&self) -> usize {
        self.content.len() + self.sig.len() + self.cert.as_ref().map_or(0, String::len)
    }

    /// Payload retained while routing/queuing, including the unsigned invite.
    pub fn retained_len(&self) -> usize {
        self.stored_len()
            .saturating_add(self.invite.as_ref().map_or(0, String::len))
    }

    pub fn valid_invite(&self) -> bool {
        self.invite.as_deref().is_none_or(valid_invite_code)
    }
}

/// Relay capabilities contain ten Crockford-base32 characters. Existing clients
/// can send lowercase; surrounding formatting is normalized before publishing.
#[must_use]
pub fn valid_invite_code(code: &str) -> bool {
    code.len() == 10
        && code
            .bytes()
            .all(|c| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&c.to_ascii_uppercase()))
}

/// An envelope after the relay put it in the Space's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Sequenced {
    pub seq: u64,
    pub prev: String,
    pub hash: String,
    pub env: Envelope,
}

impl Sequenced {
    /// The plaintext event with its place in the chain (sealed envelopes need MLS first).
    #[must_use]
    pub fn plain_event(&self) -> Option<Event> {
        let mut e = self.env.plain_event()?;
        e.seq = self.seq;
        e.prev.clone_from(&self.prev);
        e.hash.clone_from(&self.hash);
        Some(e)
    }
}

#[cfg(test)]
mod tests;
