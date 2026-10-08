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

use roda_types::{ConsentMode, TrustLevel};
use serde::{Deserialize, Serialize};

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
    pub declared: bool,
    pub access: Access,
    pub sensitivity: Sensitivity,
    /// You explicitly allowed this capability for this subject before (not counting
    /// auto-approvals).
    pub explicitly_allowed_before: bool,
    pub intent: Intent,
    pub cost_cents: i64,
    pub budget_remaining_cents: i64,
    /// Built in or verified (marketplace review). Unverified/new ones always ask at first.
    pub verified: bool,
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
    pub fn rule(self) -> Rule {
        match self {
            Verdict::Auto(r) | Verdict::Ask(r) | Verdict::Deny(r) => r,
        }
    }
}

/// Mode after trust: Listen/Suggest can't be more than Ask; Trusted needs the mark (or an
/// Autonomous agent); everything else is your setting.
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

pub fn review(r: &Request) -> Verdict {
    if !r.declared {
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
    if !r.verified && r.uses < MIN_USES_BEFORE_AUTO {
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
pub fn classify(capability: &str) -> (Access, Sensitivity) {
    use Access::*;
    use Sensitivity::*;
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
mod tests {
    use super::*;

    /// A built-in mini-app at Act, Auto mode, used a few times, right after your tap.
    fn base(cap: &str) -> Request {
        let (access, sensitivity) = classify(cap);
        Request {
            declared: true,
            access,
            sensitivity,
            explicitly_allowed_before: false,
            intent: Intent {
                fresh: true,
                matches: true,
            },
            cost_cents: 0,
            budget_remaining_cents: 500,
            verified: true,
            uses: 5,
            trust: TrustLevel::Act,
            mode: ConsentMode::Auto,
            marked_trusted: false,
        }
    }

    const ALWAYS: [&str; 7] = [
        "message.outside",
        "email.send",
        "item.delete",
        "payment",
        "share.group",
        "net:tiles.openfreemap.org",
        "net:evil.example",
    ];
    const SENSITIVE: [&str; 5] = [
        "location",
        "location.background",
        "calendar.events",
        "contacts.full",
        "health.steps",
    ];

    #[test]
    fn journey_saturday_hike_distance_is_auto_after_a_tap() {
        // You tap "Show" on the trail page: approximate location is a read that matches.
        assert_eq!(
            review(&base("location.approximate")),
            Verdict::Auto(Rule::AutoRead)
        );
    }

    #[test]
    fn journey_free_busy_from_your_message_is_auto() {
        // "when is everyone free on saturday?" → the agent wants free/busy.
        let mut r = base("calendar.freebusy");
        r.intent = Intent {
            fresh: true,
            matches: keywords_match("calendar.freebusy", "when is everyone free on saturday?"),
        };
        assert_eq!(review(&r), Verdict::Auto(Rule::AutoRead));
    }

    #[test]
    fn journey_background_grab_asks() {
        // The mini-app asks for location while you weren't doing anything.
        let mut r = base("location.approximate");
        r.intent = Intent {
            fresh: false,
            matches: false,
        };
        assert_eq!(review(&r), Verdict::Ask(Rule::NoRecentAction));
    }

    #[test]
    fn journey_intent_mismatch_asks() {
        // You said "add Lucas to the list"; the app wants your location.
        let mut r = base("location.approximate");
        r.intent = Intent {
            fresh: true,
            matches: keywords_match("location.approximate", "add Lucas to the list"),
        };
        assert_eq!(review(&r), Verdict::Ask(Rule::IntentMismatch));
    }

    #[test]
    fn journey_vote_is_a_reversible_write_in_the_space() {
        assert_eq!(
            review(&base("state.write")),
            Verdict::Auto(Rule::AutoWriteInSpace)
        );
    }

    #[test]
    fn journey_add_to_calendar_asks_in_auto_but_not_in_trusted() {
        assert_eq!(
            review(&base("calendar.add")),
            Verdict::Ask(Rule::WriteOutsideSpace)
        );
        let mut r = base("calendar.add");
        r.mode = ConsentMode::Trusted;
        r.marked_trusted = true;
        assert_eq!(review(&r), Verdict::Auto(Rule::TrustedSubject));
    }

    #[test]
    fn journey_ask_mode_always_asks() {
        let mut r = base("location.approximate");
        r.mode = ConsentMode::Ask;
        assert_eq!(review(&r), Verdict::Ask(Rule::ModeAsk));
    }

    #[test]
    fn journey_listen_and_suggest_map_to_ask() {
        for trust in [TrustLevel::Listen, TrustLevel::Suggest] {
            let mut r = base("location.approximate");
            r.trust = trust;
            r.mode = ConsentMode::Trusted;
            r.marked_trusted = true;
            assert_eq!(review(&r), Verdict::Ask(Rule::TrustBelowAct), "{trust:?}");
        }
    }

    #[test]
    fn journey_trusted_needs_the_mark() {
        let mut r = base("calendar.add");
        r.mode = ConsentMode::Trusted;
        assert_eq!(effective_mode(r.mode, r.trust, false), ConsentMode::Auto);
        assert_eq!(review(&r), Verdict::Ask(Rule::WriteOutsideSpace));
        // An Autonomous agent counts as marked.
        r.trust = TrustLevel::Autonomous;
        assert_eq!(review(&r), Verdict::Auto(Rule::TrustedSubject));
    }

    #[test]
    fn journey_new_unverified_app_asks_until_used_three_times() {
        let mut r = base("location.approximate");
        r.verified = false;
        for uses in 0..MIN_USES_BEFORE_AUTO {
            r.uses = uses;
            assert_eq!(review(&r), Verdict::Ask(Rule::AlwaysNewOrUnverified));
        }
        r.uses = MIN_USES_BEFORE_AUTO;
        assert_eq!(review(&r), Verdict::Auto(Rule::AutoRead));
    }

    #[test]
    fn journey_sensitive_asks_the_first_time_then_follows_the_mode() {
        for cap in SENSITIVE {
            assert_eq!(
                review(&base(cap)),
                Verdict::Ask(Rule::AlwaysSensitiveFirst),
                "{cap}"
            );
            let mut r = base(cap);
            r.explicitly_allowed_before = true;
            assert_eq!(
                review(&r),
                Verdict::Auto(Rule::AutoRead),
                "{cap} after you allowed it"
            );
        }
    }

    #[test]
    fn journey_over_budget_asks() {
        let mut r = base("state.write");
        r.cost_cents = 600;
        assert_eq!(review(&r), Verdict::Ask(Rule::AlwaysOverBudget));
    }

    #[test]
    fn undeclared_is_denied() {
        let mut r = base("location.approximate");
        r.declared = false;
        assert_eq!(review(&r), Verdict::Deny(Rule::NotDeclared));
    }

    /// The red line, exhaustively: every always-ask request, in every mode, trust level,
    /// intent, use count and mark, never comes back Auto.
    #[test]
    fn always_ask_list_never_auto_approves() {
        let modes = [ConsentMode::Ask, ConsentMode::Auto, ConsentMode::Trusted];
        let trusts = [
            TrustLevel::Listen,
            TrustLevel::Suggest,
            TrustLevel::Act,
            TrustLevel::Autonomous,
        ];
        let mut checked = 0;
        for cap in ALWAYS
            .iter()
            .chain(SENSITIVE.iter())
            .chain(["location.approximate", "state.write"].iter())
        {
            for &mode in &modes {
                for &trust in &trusts {
                    for marked in [false, true] {
                        for fresh in [false, true] {
                            for verified in [false, true] {
                                for uses in [0, 2, 50] {
                                    for over_budget in [false, true] {
                                        let mut r = base(cap);
                                        r.mode = mode;
                                        r.trust = trust;
                                        r.marked_trusted = marked;
                                        r.intent = Intent {
                                            fresh,
                                            matches: fresh,
                                        };
                                        r.verified = verified;
                                        r.uses = uses;
                                        if over_budget {
                                            r.cost_cents = 10_000;
                                        }
                                        let always = ALWAYS.contains(cap)
                                            || (SENSITIVE.contains(cap)
                                                && !r.explicitly_allowed_before)
                                            || over_budget
                                            || (!verified && uses < MIN_USES_BEFORE_AUTO);
                                        let v = review(&r);
                                        if always {
                                            assert!(!matches!(v, Verdict::Auto(_)), "{cap} {mode:?} {trust:?} marked={marked} fresh={fresh} verified={verified} uses={uses} over={over_budget} → {v:?}");
                                            assert!(
                                                v.rule().is_always_ask(),
                                                "{cap} should cite an always-ask rule, got {v:?}"
                                            );
                                        }
                                        checked += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 5000);
    }
}
