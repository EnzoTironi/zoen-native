//! Deterministic provider: keyword heuristics, the same answer every time. Used by the
//! tests and as the floor when no model is available (it never sees anything remote).

use crate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};

#[derive(Default, Clone, Copy)]
pub struct Mock;

fn any(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| t.contains(w))
}

impl DecisionProvider for Mock {
    fn name(&self) -> String {
        "mock".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let t = req.input.to_lowercase();
        let (said, body) = match t.split_once('\n') {
            Some((a, b)) if a.starts_with("user just said") => (a.to_string(), b.to_string()),
            _ => (String::new(), t.clone()),
        };
        let has_email = body
            .split_whitespace()
            .any(|w| w.contains('@') && w.contains('.'));
        let send_out = has_email
            || any(
                &body,
                &[
                    "send to",
                    "upload",
                    "post to",
                    "forward",
                    "webhook",
                    "http://",
                    "https://",
                    "email ",
                    "e-mail",
                    "pastebin",
                    "attacker",
                    "export all",
                    "all contacts",
                    "every contact",
                    "address book",
                ],
            );
        let personal = any(
            &body,
            &[
                "contacts",
                "address",
                "location",
                "photos",
                "messages",
                "calendar",
                "health",
                "password",
                "token",
                "phone number",
                "history",
            ],
        );
        let destructive = any(
            &body,
            &[
                "delete",
                "remove all",
                "wipe",
                "erase",
                "cancel the",
                "unsubscribe everyone",
                "transfer",
                "pay ",
                "purchase",
                "buy ",
                "refund",
            ],
        );
        let mut out = DecisionResponse::default();
        for q in &req.questions {
            let a = match (q, q.name()) {
                (_, "matches_user_intent") => {
                    let overlap = said
                        .split(|c: char| !c.is_alphanumeric())
                        .filter(|w| w.len() > 3)
                        .any(|w| body.contains(w));
                    Answer::Predicate {
                        p: if said.is_empty() {
                            0.3
                        } else if overlap {
                            0.85
                        } else {
                            0.35
                        },
                    }
                }
                (_, "severity") => Answer::Score {
                    score: if destructive && send_out {
                        2.6
                    } else if destructive || (send_out && personal) {
                        1.8
                    } else if send_out {
                        1.0
                    } else {
                        0.2
                    },
                    confidence: 0.7,
                },
                (_, "is_irreversible") => Answer::Predicate {
                    p: if destructive { 0.8 } else { 0.1 },
                },
                (_, "exfiltrates_personal_data") => Answer::Predicate {
                    p: if send_out && personal {
                        0.9
                    } else if send_out {
                        0.4
                    } else {
                        0.05
                    },
                },
                (_, "outside_space") => Answer::Predicate {
                    p: if send_out { 0.8 } else { 0.1 },
                },
                (_, "indirect_prompt_injection") => Answer::Predicate {
                    p: if any(
                        &body,
                        &[
                            "assistant",
                            "ai model",
                            "instruction",
                            "you must",
                            "do not tell",
                            "secretly",
                            "llm",
                            "as an ai",
                        ],
                    ) {
                        0.8
                    } else {
                        0.05
                    },
                },
                (_, "hidden_instructions") => Answer::Predicate {
                    p: if any(
                        &body,
                        &[
                            "<!--",
                            "display:none",
                            "font-size:0",
                            "base64",
                            "white text",
                            "color:#fff",
                        ],
                    ) {
                        0.7
                    } else {
                        0.05
                    },
                },
                (_, "malicious_behavior") => Answer::Predicate {
                    p: if any(
                        &body,
                        &[
                            "exfil",
                            "steal",
                            "miner",
                            "fingerprint",
                            "track",
                            "obfuscat",
                        ],
                    ) {
                        0.8
                    } else {
                        0.05
                    },
                },
                (_, "data_harvesting") => Answer::Predicate {
                    p: if any(
                        &body,
                        &[
                            "contacts.full",
                            "health",
                            "all photos",
                            "background location",
                            "location.background",
                        ],
                    ) && any(&body, &["allowed-domains: [\"", "net:"])
                    {
                        0.8
                    } else {
                        0.1
                    },
                },
                (Question::Choice { .. }, "policy_category") => Answer::Choice {
                    choice: if any(&body, &["bet", "casino", "wager"]) {
                        "gambling"
                    } else if any(&body, &["payment", "wallet", "crypto"]) {
                        "finance"
                    } else if any(&body, &["steps", "health"]) {
                        "health"
                    } else {
                        "social"
                    }
                    .into(),
                    confidence: 0.7,
                },
                (_, "harmful_content") => Answer::Score {
                    score: if any(&body, &["kill", "hate", "stupid", "idiot", "worthless"]) {
                        2.2
                    } else {
                        0.1
                    },
                    confidence: 0.7,
                },
                (_, "pii_or_secret_leak") => Answer::Predicate {
                    p: if any(
                        &body,
                        &[
                            "[phone]",
                            "[user]@",
                            "[cpf]",
                            "lives at",
                            "home address",
                            "password is",
                            "diagnos",
                            "passport number",
                            "rua ",
                            " street",
                        ],
                    ) {
                        0.85
                    } else {
                        0.05
                    },
                },
                _ => continue,
            };
            out.answers.insert(q.name().into(), a);
        }
        Ok(out)
    }
}
