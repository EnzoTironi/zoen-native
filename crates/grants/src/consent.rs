//! # Consent reviewer (Ask / Auto / Trusted)
//!
//! Decides whether a mini-app or agent request needs Zoen's consent sheet. Pure and
//! deterministic: the same inputs give the same answer on every device, and the always-ask
//! list is checked before the mode is even looked at, so no mode can switch it off.
//!
//! iOS system prompts are outside this: they appear once per permission for the Zoen app
//! no matter what. This only decides about Zoen's own per-mini-app sheet.
//!
//! The one concept with trust levels: Listen/Suggest → Ask, Act → the mode you set
//! (Auto by default), Autonomous → Trusted. `Trusted` set on a subject that you haven't
//! marked trusted falls back to Auto.

use serde::{Deserialize, Serialize};
use types::{ConsentMode, TrustLevel};

/// A new or unverified mini-app/agent asks every time until you've used it this many times.
pub const MIN_USES_BEFORE_AUTO: u32 = 3;

/// What the request does with your data.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Reads something on this device for the mini-app (location, free/busy, a picked photo).
    Read,
    /// A reversible change inside the Space (a vote, a list item).
    WriteInSpace,
    /// A change outside the Space that you can undo (an event in your own calendar).
    WriteOutside,
    /// Your personal data into the group's shared state.
    ShareToGroup,
    /// Data to a server or domain outside Zoen (including network access for a mini-app).
    External,
    /// Sending a message or posting outside the current Space.
    PostOutside,
    /// Deleting, sending email, anything that can't be taken back.
    Irreversible,
    /// Payments or purchases.
    Payment,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensitivity {
    Normal,
    /// Health, full contacts, full calendar details, precise or background location.
    Sensitive,
}

/// Whether this capability is actually declared by the subject's manifest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(from = "bool", into = "bool")]
pub enum Declaration {
    Missing,
    Declared,
}
impl From<bool> for Declaration {
    fn from(value: bool) -> Self {
        if value {
            Self::Declared
        } else {
            Self::Missing
        }
    }
}
impl From<Declaration> for bool {
    fn from(value: Declaration) -> Self {
        value == Declaration::Declared
    }
}

/// Marketplace/built-in verification, separate from an owner's consent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(from = "bool", into = "bool")]
pub enum Verification {
    Unverified,
    Verified,
}
impl From<bool> for Verification {
    fn from(value: bool) -> Self {
        if value {
            Self::Verified
        } else {
            Self::Unverified
        }
    }
}
impl From<Verification> for bool {
    fn from(value: Verification) -> Self {
        value == Verification::Verified
    }
}

/// Was there a user action behind this request?
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    /// A tap in this mini-app or your own message in this Space, moments ago.
    pub fresh: bool,
    /// The capability fits that action (a tap on the mini-app always fits; for a message,
    /// the on-device model's judgment or the keyword fallback).
    pub matches: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Declared in the manifest with a purpose.
    pub declared: Declaration,
    pub access: Access,
    pub sensitivity: Sensitivity,
    /// You explicitly allowed this capability for this subject before (not counting
    /// auto-approvals).
    pub explicitly_allowed_before: bool,
    pub intent: Intent,
    pub cost_cents: i64,
    pub budget_remaining_cents: i64,
    /// Built in or verified (marketplace review). Unverified/new ones always ask at first.
    pub verified: Verification,
    /// Times you've used the mini-app or agent.
    pub uses: u32,
    /// The subject's trust level in this Space.
    pub trust: TrustLevel,
    /// The mode resolved from your settings (mini-app > agent > Space > global).
    pub mode: ConsentMode,
    /// You marked this mini-app or agent as trusted.
    pub marked_trusted: bool,
}

/// The policy row that decided. Stable ids: they go in the log and the toast.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    NotDeclared,
    AlwaysPostOutside,
    AlwaysIrreversible,
    AlwaysPayment,
    AlwaysOverBudget,
    AlwaysShareToGroup,
    AlwaysExternal,
    AlwaysSensitiveFirst,
    AlwaysNewOrUnverified,
    ModeAsk,
    TrustBelowAct,
    NoRecentAction,
    IntentMismatch,
    WriteOutsideSpace,
    AutoRead,
    AutoWriteInSpace,
    TrustedSubject,
}

impl Rule {
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Rule::NotDeclared => "not-declared",
            Rule::AlwaysPostOutside => "always:post-outside",
            Rule::AlwaysIrreversible => "always:irreversible",
            Rule::AlwaysPayment => "always:payment",
            Rule::AlwaysOverBudget => "always:over-budget",
            Rule::AlwaysShareToGroup => "always:share-to-group",
            Rule::AlwaysExternal => "always:external",
            Rule::AlwaysSensitiveFirst => "always:sensitive-first",
            Rule::AlwaysNewOrUnverified => "always:new-or-unverified",
            Rule::ModeAsk => "mode:ask",
            Rule::TrustBelowAct => "trust:below-act",
            Rule::NoRecentAction => "auto:no-recent-action",
            Rule::IntentMismatch => "auto:intent-mismatch",
            Rule::WriteOutsideSpace => "auto:write-outside-space",
            Rule::AutoRead => "auto:read-in-intent",
            Rule::AutoWriteInSpace => "auto:reversible-write-in-space",
            Rule::TrustedSubject => "trusted",
        }
    }

    #[must_use]
    pub fn is_always_ask(self) -> bool {
        self.id().starts_with("always:")
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Approve without the sheet (a non-blocking toast with Undo).
    Auto(Rule),
    /// Show Zoen's consent sheet.
    Ask(Rule),
    /// Refuse outright (the app asked for something it never declared).
    Deny(Rule),
}

impl Verdict {
    #[must_use]
    pub fn rule(self) -> Rule {
        match self {
            Verdict::Auto(r) | Verdict::Ask(r) | Verdict::Deny(r) => r,
        }
    }
}

/// Mode after trust: Listen/Suggest can't be more than Ask; Trusted needs the mark (or an
/// Autonomous agent); everything else is your setting.
#[must_use]
pub fn effective_mode(
    setting: ConsentMode,
    trust: TrustLevel,
    marked_trusted: bool,
) -> ConsentMode {
    if trust < TrustLevel::Act {
        return ConsentMode::Ask;
    }
    let marked = marked_trusted || trust == TrustLevel::Autonomous;
    match setting {
        ConsentMode::Trusted if !marked => ConsentMode::Auto,
        m => m,
    }
}

#[must_use]
pub fn review(r: &Request) -> Verdict {
    if r.declared == Declaration::Missing {
        return Verdict::Deny(Rule::NotDeclared);
    }
    // 1. Always ask, in every mode. Order matters only for which reason you see.
    match r.access {
        Access::PostOutside => return Verdict::Ask(Rule::AlwaysPostOutside),
        Access::Irreversible => return Verdict::Ask(Rule::AlwaysIrreversible),
        Access::Payment => return Verdict::Ask(Rule::AlwaysPayment),
        Access::ShareToGroup => return Verdict::Ask(Rule::AlwaysShareToGroup),
        Access::External => return Verdict::Ask(Rule::AlwaysExternal),
        _ => {}
    }
    if r.cost_cents > r.budget_remaining_cents.max(0) {
        return Verdict::Ask(Rule::AlwaysOverBudget);
    }
    if r.sensitivity == Sensitivity::Sensitive && !r.explicitly_allowed_before {
        return Verdict::Ask(Rule::AlwaysSensitiveFirst);
    }
    if r.verified == Verification::Unverified && r.uses < MIN_USES_BEFORE_AUTO {
        return Verdict::Ask(Rule::AlwaysNewOrUnverified);
    }
    // 2. The mode.
    match effective_mode(r.mode, r.trust, r.marked_trusted) {
        ConsentMode::Ask => Verdict::Ask(if r.trust < TrustLevel::Act {
            Rule::TrustBelowAct
        } else {
            Rule::ModeAsk
        }),
        ConsentMode::Trusted => Verdict::Auto(Rule::TrustedSubject),
        ConsentMode::Auto => {
            // No background grabs: something you just did has to be behind it.
            if !r.intent.fresh {
                return Verdict::Ask(Rule::NoRecentAction);
            }
            if !r.intent.matches {
                return Verdict::Ask(Rule::IntentMismatch);
            }
            match r.access {
                Access::Read => Verdict::Auto(Rule::AutoRead),
                Access::WriteInSpace => Verdict::Auto(Rule::AutoWriteInSpace),
                _ => Verdict::Ask(Rule::WriteOutsideSpace),
            }
        }
    }
}

/// How a capability string reads to the reviewer. Unknown ones are treated as sensitive
/// writes outside the Space, so they ask.
#[must_use]
pub fn classify(capability: &str) -> (Access, Sensitivity) {
    use Access::{
        External, Irreversible, Payment, PostOutside, Read, ShareToGroup, WriteInSpace,
        WriteOutside,
    };
    use Sensitivity::{Normal, Sensitive};
    match capability {
        "location.approximate"
        | "photos.pick"
        | "camera.capture"
        | "calendar.freebusy"
        | "contacts.pick" => (Read, Normal),
        "location"
        | "location.background"
        | "calendar.events"
        | "contacts.full"
        | "health.steps" => (Read, Sensitive),
        "calendar.add" => (WriteOutside, Normal),
        "state.write" => (WriteInSpace, Normal),
        "share.group" => (ShareToGroup, Normal),
        "message.outside" => (PostOutside, Normal),
        "email.send" | "item.delete" => (Irreversible, Normal),
        "payment" => (Payment, Normal),
        c if c.starts_with("net:") => (External, Normal),
        _ => (WriteOutside, Sensitive),
    }
}

/// Deterministic intent fallback: does the message talk about what the capability gives?
/// (The on-device model, when there is one, answers this instead.)
#[must_use]
pub fn keywords_match(capability: &str, message: &str) -> bool {
    let m = message.to_lowercase();
    let words: &[&str] = match capability.split('.').next().unwrap_or("") {
        "location" => &[
            "far",
            "near",
            "distance",
            "where",
            "close to",
            "longe",
            "perto",
            "distância",
            "distancia",
            "onde",
        ],
        "calendar" => &[
            "free", "busy", "when", "calendar", "schedule", "saturday", "sunday", "livre",
            "ocupad", "quando", "agenda", "sábado", "sabado", "domingo",
        ],
        "photos" | "camera" => &["photo", "pic", "picture", "album", "foto", "álbum", "album"],
        "contacts" => &["invite", "contact", "number", "convid", "contato", "número"],
        "health" => &["steps", "walk", "passos", "caminh"],
        _ => &[],
    };
    words.iter().any(|w| m.contains(w))
}

#[cfg(test)]
mod tests;
