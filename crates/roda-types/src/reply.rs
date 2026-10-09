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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReplyRef {
    /// Event hash of the message being answered (the thread root for thread replies).
    pub to: String,
    /// `true` = in the thread under `to`; `false` = inline, quoting `to`.
    #[serde(default)]
    pub thread: bool,
}

impl ReplyRef {
    pub fn inline(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            thread: false,
        }
    }
    pub fn thread(root: impl Into<String>) -> Self {
        Self {
            to: root.into(),
            thread: true,
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
        };
        assert!(!serde_json::to_string(&plain).unwrap().contains("reply"));
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
        })
        .unwrap();
        let OldBody::MessagePosted { text } = serde_json::from_str(&json).unwrap();
        assert_eq!(text, "sim");
    }
}
