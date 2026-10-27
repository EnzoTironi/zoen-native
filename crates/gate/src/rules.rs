use crate::{redact, Check, Outcome, Point};
use grants::consent::{self, Access, Verdict};

// ───────────────────────────── hard rules ─────────────────────────────

const DESTRUCTIVE: [&str; 12] = [
    "rm -rf",
    "drop table",
    "drop database",
    "truncate table",
    "format disk",
    "mkfs",
    "delete all",
    "wipe all",
    "delete every",
    "factory reset",
    ":(){",
    "del /f /s",
];
const INJECTION: [&str; 14] = [
    "ignore previous instructions",
    "ignore all previous",
    "ignore the previous",
    "disregard your instructions",
    "disregard previous",
    "forget your instructions",
    "new instructions:",
    "system prompt",
    "you are now",
    "ignore prior",
    "ignore todas as instru",
    "ignore as instru",
    "assistant: sure",
    "begin hidden",
];
const TRACKING: [&str; 6] = [
    "document.cookie",
    "navigator.sendbeacon",
    "keylogger",
    "localstorage.getitem('token",
    "indexeddb.open('wallet",
    "crypto miner",
];
const CREDENTIAL_PATHS: [&str; 7] = [
    "auth.json",
    "id_rsa",
    ".ssh/",
    "keychain",
    ".aws/credentials",
    ".env",
    "passwords.txt",
];

fn has(text: &str, needles: &[&str]) -> bool {
    let t = text.to_lowercase();
    needles.iter().any(|n| t.contains(n))
}

/// Zero-width and bidi control characters, used to hide instructions.
fn hidden_chars(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{E0000}'..='\u{E007F}'))
}

#[must_use]
pub fn hard_rules(c: &Check) -> Option<(Outcome, String)> {
    let deny = |r: &str| Some((Outcome::Block, format!("hard:{r}")));
    match c.point {
        Point::ToolCall => {
            if redact::contains_secret(&c.text) || has(&c.text, &CREDENTIAL_PATHS) {
                return deny("credential");
            }
            if has(&c.text, &DESTRUCTIVE) {
                return deny("destructive");
            }
            if let Some(r) = &c.consent {
                match consent::review(r) {
                    Verdict::Deny(rule) => return Some((Outcome::Block, rule.id().into())),
                    Verdict::Ask(rule) if rule.is_always_ask() => {
                        return Some((Outcome::Ask, rule.id().into()))
                    }
                    _ => {}
                }
            } else if let Some(cap) = &c.capability {
                // No full consent context: the access class alone still enforces the list.
                let (access, sens) = consent::classify(cap);
                let rule = match access {
                    Access::PostOutside => Some("always:post-outside"),
                    Access::Irreversible => Some("always:irreversible"),
                    Access::Payment => Some("always:payment"),
                    Access::ShareToGroup => Some("always:share-to-group"),
                    Access::External => Some("always:external"),
                    _ if sens == consent::Sensitivity::Sensitive => Some("always:sensitive-first"),
                    _ => None,
                };
                if let Some(r) = rule {
                    return Some((Outcome::Ask, r.into()));
                }
            }
            if c.irreversible {
                return Some((Outcome::Ask, "always:irreversible".into()));
            }
            None
        }
        Point::Inbound | Point::ConversationMessage => {
            if hidden_chars(&c.text) {
                return deny("hidden-characters");
            }
            if has(&c.text, &INJECTION) {
                return deny("injection-marker");
            }
            None
        }
        Point::Bundle => {
            if has(&c.text, &TRACKING) {
                return deny("tracking-api");
            }
            if redact::contains_secret(&c.text) {
                return deny("embedded-secret");
            }
            if c.text.contains("undeclared-network: yes") {
                return deny("undeclared-network");
            }
            None
        }
        Point::Outbound => {
            if redact::contains_secret(&c.text) {
                return deny("secret");
            }
            if redact::contains_card(&c.text) {
                return deny("card-number");
            }
            None
        }
    }
}
