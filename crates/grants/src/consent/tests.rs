use super::*;

/// A built-in mini-app at Act, Auto mode, used a few times, right after your tap.
fn base(cap: &str) -> Request {
    let (access, sensitivity) = classify(cap);
    Request {
        declared: true.into(),
        access,
        sensitivity,
        explicitly_allowed_before: false,
        intent: Intent {
            fresh: true,
            matches: true,
        },
        cost_cents: 0,
        budget_remaining_cents: 500,
        verified: true.into(),
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
    r.verified = false.into();
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
    r.declared = false.into();
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
                                    r.verified = verified.into();
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

#[test]
fn typed_manifest_and_verification_preserve_boolean_json() {
    for (declaration, wire) in [
        (Declaration::Missing, "false"),
        (Declaration::Declared, "true"),
    ] {
        assert_eq!(serde_json::to_string(&declaration).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<Declaration>(wire).unwrap(),
            declaration
        );
    }
    for (verification, wire) in [
        (Verification::Unverified, "false"),
        (Verification::Verified, "true"),
    ] {
        assert_eq!(serde_json::to_string(&verification).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<Verification>(wire).unwrap(),
            verification
        );
    }
    assert!(serde_json::from_str::<Declaration>("null").is_err());
    assert!(serde_json::from_str::<Verification>("\"true\"").is_err());
}
