//! Replies and threads: a message can point at an earlier message in the same Space.
//!
//! The link rides inside `EventBody::MessagePosted` as an optional `reply` field, so it is
//! signed with the message and needs no extra event. Older readers skip the unknown field
//! and simply see a plain message; nothing is re-encoded, so the bytes still verify.
//!
//! - `thread == false`: an inline reply (Telegram-style). The message stays in the main
//!   timeline and shows a quote of `to`.
//! - `thread == true`: a reply in the thread rooted at `to` (Slack-style). It lives in the
//!   thread view; the main timeline only shows "N replies" under the root.
//!
//! `to` names the target by its event hash (the timeline entry id), which is stable from
//! the moment the author signs it. A thread reply always points at the root, never at
//! another thread reply, so threads stay one level deep.

use serde::{Deserialize, Serialize};

/// Signed request for one App and its observed document head. It narrows existing
/// authority; it grants no membership, trust or permission to execute a tool.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRequest {
    pub item_id: String,
    pub version: u32,
    pub sha256: String,
    pub request_id: String,
    pub text_sha256: String,
    /// Exact operator selected by the owner; absent only on older signed requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Exact publication intent, retaining retry arguments without another ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_sha256: Option<String>,
}

impl ArtifactRequest {
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        let token = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        token(&self.item_id)
            && token(&self.request_id)
            && self.agent_id.as_deref().is_none_or(token)
            && self.version > 0
            && digest(&self.sha256)
            && digest(&self.text_sha256)
            && self.intent_sha256.as_deref().is_none_or(digest)
    }
}

/// Exact literal selection from a signed source revision. Coordinates are half-open
/// UTF-16 source units, ordered and nonoverlapping. Syntax gaps are allowed; omitted
/// visible text is not. Zoen bounds: 4096 excerpt units and 128 spans.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectedQuote {
    pub source_id: String,
    pub revision: String,
    pub spans: Vec<QuoteSpan>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuoteSpan {
    pub start_utf16: u32,
    pub end_utf16: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReplyRef {
    /// Event hash of the message being answered (the thread root for thread replies).
    pub to: String,
    /// `true` = in the thread under `to`; `false` = inline, quoting `to`.
    #[serde(default)]
    pub thread: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<Box<SelectedQuote>>,
}

impl ReplyRef {
    pub fn inline(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            thread: false,
            artifact: None,
            quote: None,
        }
    }
    pub fn thread(root: impl Into<String>) -> Self {
        Self {
            to: root.into(),
            thread: true,
            artifact: None,
            quote: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventBody;

    #[test]
    fn old_messages_decode_without_a_reply_and_new_ones_round_trip() {
        let old = r#"{"MessagePosted":{"message":"m","text":"oi","attaches":null}}"#;
        let EventBody::MessagePosted { reply, .. } = serde_json::from_str(old).unwrap() else {
            panic!()
        };
        assert_eq!(reply, None);

        let new = EventBody::MessagePosted {
            message: "m2".into(),
            text: "sim".into(),
            attaches: None,
            reply: Some(ReplyRef::thread("h1")),
            media: None,
            forwarded: false,
        };
        let json = serde_json::to_string(&new).unwrap();
        assert!(json.contains(r#""reply":{"to":"h1","thread":true}"#));
        assert_eq!(serde_json::from_str::<EventBody>(&json).unwrap(), new);
        // A plain message serialises exactly as before (no `reply` key), so its bytes and
        // hash are unchanged for older peers.
        let plain = EventBody::MessagePosted {
            message: "m".into(),
            text: "oi".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        };
        assert!(!serde_json::to_string(&plain).unwrap().contains("reply"));
    }

    #[test]
    fn artifact_scope_is_typed_and_legacy_reply_bytes_are_identical() {
        assert_eq!(
            serde_json::to_string(&ReplyRef::thread("root")).unwrap(),
            r#"{"to":"root","thread":true}"#
        );
        let mut reply = ReplyRef::thread("root");
        reply.artifact = Some(ArtifactRequest {
            item_id: "item".into(),
            version: 1,
            sha256: "ab".repeat(32),
            request_id: "request".into(),
            text_sha256: "cd".repeat(32),
            agent_id: None,
            intent_sha256: None,
        });
        assert!(reply.artifact.as_ref().unwrap().is_well_formed());
        let mut value = serde_json::to_value(&reply).unwrap();
        assert_eq!(
            serde_json::from_value::<ReplyRef>(value.clone()).unwrap(),
            reply
        );
        value["artifact"]["grant"] = serde_json::json!("all");
        assert!(serde_json::from_value::<ReplyRef>(value).is_err());
    }

    #[test]
    fn old_message_consumer_cannot_decode_scoped_request_as_chat() {
        // The old decoder (event_log::decode_body) turns this decode failure into
        // Unsupported while retaining signed raw bytes. It never sees a prompt.
        #[derive(Deserialize)]
        enum LegacyBody {
            MessagePosted { text: String },
        }
        let mut reply = ReplyRef::thread("root");
        reply.artifact = Some(ArtifactRequest {
            item_id: "item".into(),
            version: 1,
            sha256: "ab".repeat(32),
            request_id: "request".into(),
            text_sha256: "cd".repeat(32),
            agent_id: None,
            intent_sha256: None,
        });
        let scoped = serde_json::to_vec(&EventBody::AppEditRequested {
            message: "m".into(),
            text: "Only this App".into(),
            reply,
        })
        .unwrap();
        assert!(serde_json::from_slice::<LegacyBody>(&scoped).is_err());
        let LegacyBody::MessagePosted { text } = serde_json::from_str(
            r#"{"MessagePosted":{"message":"m","text":"legacy","attaches":null}}"#,
        )
        .unwrap();
        assert_eq!(text, "legacy");
    }

    #[test]
    fn artifact_recipient_is_signed_optional_and_rejected_by_old_scoped_consumers() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LegacyArtifact {
            #[serde(rename = "item_id")]
            _item: String,
            #[serde(rename = "version")]
            _version: u32,
            #[serde(rename = "sha256")]
            _sha: String,
            #[serde(rename = "request_id")]
            _request: String,
            #[serde(rename = "text_sha256")]
            _text: String,
        }
        let old = format!(
            r#"{{"item_id":"item","version":1,"sha256":"{}","request_id":"request","text_sha256":"{}"}}"#,
            "ab".repeat(32),
            "cd".repeat(32)
        );
        let mut scoped: ArtifactRequest = serde_json::from_str(&old).unwrap();
        assert!(scoped.agent_id.is_none());
        assert_eq!(serde_json::to_string(&scoped).unwrap(), old);
        scoped.agent_id = Some("operator".into());
        assert!(scoped.is_well_formed());
        let json = serde_json::to_string(&scoped).unwrap();
        assert!(serde_json::from_str::<LegacyArtifact>(&old).is_ok());
        assert!(serde_json::from_str::<LegacyArtifact>(&json).is_err());
        scoped.agent_id = Some("../operator".into());
        assert!(!scoped.is_well_formed());
    }

    #[test]
    fn an_older_reader_skips_the_reply_field() {
        #[derive(Deserialize)]
        enum OldBody {
            MessagePosted { text: String },
        }
        let json = serde_json::to_string(&EventBody::MessagePosted {
            message: "m".into(),
            text: "sim".into(),
            attaches: None,
            reply: Some(ReplyRef::inline("h1")),
            media: None,
            forwarded: false,
        })
        .unwrap();
        let OldBody::MessagePosted { text } = serde_json::from_str(&json).unwrap();
        assert_eq!(text, "sim");
    }
}

#[cfg(test)]
mod selected_quote_tests {
    use super::*;
    #[test]
    fn optional_quote_preserves_legacy_signed_reply_bytes() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_string(&ReplyRef::inline("root"))?,
            r#"{"to":"root","thread":false}"#
        );
        let mut reference = ReplyRef::inline("root");
        reference.quote = Some(Box::new(SelectedQuote {
            source_id: "root".into(),
            revision: "edit".into(),
            spans: vec![QuoteSpan {
                start_utf16: 3,
                end_utf16: 5,
            }],
            text: "😀".into(),
        }));
        let bytes = serde_json::to_vec(&reference)?;
        assert_eq!(serde_json::from_slice::<ReplyRef>(&bytes)?, reference);
        let legacy: ReplyRef = serde_json::from_str(r#"{"to":"root","thread":false}"#)?;
        assert!(legacy.quote.is_none());
        Ok(())
    }
}
