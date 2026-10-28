use super::super::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const LIVE: &str = include_str!("../fixtures/clef-live-synthetic-2026-10-21.json");
fn request() -> DecisionRequest {
    DecisionRequest {
        input: "Synthetic QA: 2 + 2 = 4. This is mathematics. Impact: none.".into(),
        questions: vec![
            Question::Predicate {
                name: "truth".into(),
                instructions: "Does 2 + 2 equal 4?".into(),
            },
            Question::Choice {
                name: "topic".into(),
                instructions: "Which topic matches?".into(),
                choices: vec![
                    ("math".into(), "Mathematics".into()),
                    ("weather".into(), "Weather".into()),
                ],
            },
            Question::Score {
                name: "severity".into(),
                instructions: "Rate the impact.".into(),
                levels: ["None", "Minor", "Major", "Critical"]
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            },
        ],
    }
}

struct Transport {
    reply: String,
    requests: Mutex<Vec<Value>>,
}
impl HttpTransport for Transport {
    fn post_json(&self, _: &str, _: Option<&str>, body: &str) -> Result<String, String> {
        self.requests
            .lock()
            .unwrap()
            .push(serde_json::from_str(body).unwrap());
        Ok(self.reply.clone())
    }
}
fn provider(reply: String) -> (Clef, Arc<Transport>) {
    let transport = Arc::new(Transport {
        reply,
        requests: Mutex::default(),
    });
    (
        Clef {
            transport: transport.clone(),
            account: "synthetic-account".into(),
            token: "synthetic-token".into(),
            model: ClefModel::Flash,
        },
        transport,
    )
}

#[test]
fn clef_contract_retained_real_reply_uses_existing_system_one_answer_mapping() {
    let req = request();
    let (clef, transport) = provider(LIVE.into());
    let actual = clef.decide(&req).unwrap();
    assert_eq!(
        actual,
        SystemOneHttp::parse(
            &req,
            &serde_json::from_str::<Value>(LIVE).unwrap()["result"].to_string()
        )
        .unwrap()
    );
    assert_eq!(actual.answers["truth"], Answer::Predicate { p: 0.9518 });
    assert_eq!(
        actual.answers["topic"],
        Answer::Choice {
            choice: "math".into(),
            confidence: 0.9651
        }
    );
    assert_eq!(
        actual.answers["severity"],
        Answer::Score {
            score: 0.0969,
            confidence: 0.8669
        }
    );
    let calls = transport.requests.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["state"], req.input);
    assert_eq!(calls[0]["model"], "clef-flash");
}

#[test]
fn clef_contract_invalid_envelopes_answers_and_distributions_are_refused() {
    let original: Value = serde_json::from_str(LIVE).unwrap();
    for case in [
        "success-false",
        "missing-success",
        "error-with-result",
        "missing-error-array",
        "missing-answer",
        "extra-answer",
        "wrong-type",
        "negative-noul",
        "large-noul",
        "unknown-choice",
        "nonwinning-choice",
        "missing-confidence",
        "negative-confidence",
        "large-confidence",
        "extra-probability",
        "missing-probability",
        "negative-probability",
        "bad-sum",
        "large-score",
        "inconsistent-score",
        "legend-drift",
        "missing-usage",
        "fractional-usage",
        "unknown-model",
        "wrong-configured-model",
    ] {
        let mut v = original.clone();
        match case {
            "success-false" => v["success"] = json!(false),
            "missing-success" => {
                v.as_object_mut().unwrap().remove("success");
            }
            "error-with-result" => {
                v["errors"] = json!([{"code": 10000, "message": "synthetic refusal"}]);
            }
            "missing-error-array" => {
                v.as_object_mut().unwrap().remove("errors");
            }
            "missing-answer" => {
                v["result"]["answers"]
                    .as_object_mut()
                    .unwrap()
                    .remove("truth");
            }
            "extra-answer" => v["result"]["answers"]["extra"] = json!({"type":"noul", "noul":0.01}),
            "wrong-type" => v["result"]["answers"]["truth"]["type"] = json!("choice"),
            "negative-noul" => v["result"]["answers"]["truth"]["noul"] = json!(-0.01),
            "large-noul" => v["result"]["answers"]["truth"]["noul"] = json!(1.01),
            "unknown-choice" => v["result"]["answers"]["topic"]["choice"] = json!("other"),
            "nonwinning-choice" => v["result"]["answers"]["topic"]["choice"] = json!("weather"),
            "missing-confidence" => {
                v["result"]["answers"]["topic"]
                    .as_object_mut()
                    .unwrap()
                    .remove("confidence");
            }
            "negative-confidence" => v["result"]["answers"]["topic"]["confidence"] = json!(-1),
            "large-confidence" => v["result"]["answers"]["topic"]["confidence"] = json!(2),
            "extra-probability" => {
                v["result"]["answers"]["topic"]["probabilities"]["other"] = json!(0);
            }
            "missing-probability" => {
                v["result"]["answers"]["topic"]["probabilities"]
                    .as_object_mut()
                    .unwrap()
                    .remove("weather");
            }
            "negative-probability" => {
                v["result"]["answers"]["topic"]["probabilities"]["weather"] = json!(-0.01);
            }
            "bad-sum" => v["result"]["answers"]["topic"]["probabilities"]["weather"] = json!(0.1),
            "large-score" => v["result"]["answers"]["severity"]["score"] = json!(4),
            "inconsistent-score" => v["result"]["answers"]["severity"]["score"] = json!(1.5),
            "legend-drift" => v["result"]["answers"]["severity"]["legend"]["0"] = json!("Changed"),
            "missing-usage" => {
                v["result"].as_object_mut().unwrap().remove("usage");
            }
            "fractional-usage" => v["result"]["usage"]["input_tokens"] = json!(3.5),
            "unknown-model" => v["result"]["model"] = json!("unrequested"),
            "wrong-configured-model" => v["result"]["model"] = json!("clef"),
            _ => unreachable!(),
        }
        let (clef, transport) = provider(v.to_string());
        assert!(clef.decide(&request()).is_err(), "must refuse {case}");
        assert_eq!(
            transport.requests.lock().unwrap().len(),
            1,
            "never automatically retry {case}"
        );
    }
}

#[test]
fn clef_contract_invalid_request_never_calls_transport() {
    for case in [
        "empty",
        "oversize",
        "empty-questions",
        "many-questions",
        "duplicate-name",
        "invalid-name",
        "long-name",
        "blank-instructions",
        "one-choice",
        "duplicate-choice",
        "one-level",
        "many-levels",
        "body-size",
    ] {
        let mut req = request();
        match case {
            "empty" => req.input.clear(),
            "oversize" => req.input = "🙂".repeat(6001),
            "empty-questions" => req.questions.clear(),
            "many-questions" => req.questions = vec![req.questions[0].clone(); 65],
            "duplicate-name" => req.questions.push(req.questions[0].clone()),
            "invalid-name" => {
                if let Question::Predicate { name, .. } = &mut req.questions[0] {
                    *name = "a/b".into();
                }
            }
            "long-name" => {
                if let Question::Predicate { name, .. } = &mut req.questions[0] {
                    *name = "a".repeat(101);
                }
            }
            "blank-instructions" => {
                if let Question::Predicate { instructions, .. } = &mut req.questions[0] {
                    *instructions = " ".into();
                }
            }
            "one-choice" => {
                if let Question::Choice { choices, .. } = &mut req.questions[1] {
                    choices.truncate(1);
                }
            }
            "duplicate-choice" => {
                if let Question::Choice { choices, .. } = &mut req.questions[1] {
                    choices[1] = choices[0].clone();
                }
            }
            "one-level" => {
                if let Question::Score { levels, .. } = &mut req.questions[2] {
                    levels.truncate(1);
                }
            }
            "many-levels" => {
                if let Question::Score { levels, .. } = &mut req.questions[2] {
                    *levels = vec!["level".into(); 11];
                }
            }
            "body-size" => {
                req.questions = (0..64)
                    .map(|i| Question::Predicate {
                        name: format!("q{i}"),
                        instructions: "🙂".repeat(6000),
                    })
                    .collect();
            }
            _ => unreachable!(),
        }
        let (clef, transport) = provider(LIVE.into());
        assert!(clef.decide(&req).is_err(), "must refuse {case}");
        assert!(
            transport.requests.lock().unwrap().is_empty(),
            "must not dispatch {case}"
        );
    }
}

fn inbound_reply() -> String {
    json!({"success":true,"errors":[],"result":{"model":"clef-flash","usage":{"input_tokens":10,"output_tokens":0},"answers":{"indirect_prompt_injection":{"type":"noul","noul":0.01},"hidden_instructions":{"type":"noul","noul":0.01}}}}).to_string()
}

#[test]
fn clef_contract_gate_oversize_context_never_judges_only_a_prefix() {
    let (clef, transport) = provider(inbound_reply());
    let gate = Gate {
        remote: Some(Box::new(clef)),
        ..Gate::default()
    };
    let mut check = Check::new(Point::Inbound, "x".repeat(6000));
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Allow);
    assert_eq!(transport.requests.lock().unwrap()[0]["state"], check.text);
    check.text.push('x');
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Ask);
    check.irreversible = true;
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Block);
    check.text = "plain".into();
    check.user_intent = Some("x".repeat(6000));
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Block);
    assert_eq!(transport.requests.lock().unwrap().len(), 1);
}

#[test]
fn clef_contract_gate_invalid_probability_obeys_existing_fail_policy() {
    let mut reply: Value = serde_json::from_str(&inbound_reply()).unwrap();
    reply["result"]["answers"]["indirect_prompt_injection"]["noul"] = json!(-0.1);
    let (clef, transport) = provider(reply.to_string());
    let gate = Gate {
        remote: Some(Box::new(clef)),
        ..Gate::default()
    };
    let mut check = Check::new(Point::Inbound, "Synthetic ordinary content");
    let d = gate.check(&check);
    assert_eq!(d.outcome, Outcome::Ask);
    assert!(d.rule.starts_with("fail:"));
    check.irreversible = true;
    assert_eq!(gate.check(&check).outcome, Outcome::Block);
    assert_eq!(transport.requests.lock().unwrap().len(), 2);
}

#[test]
fn clef_contract_complete_text_and_questions_share_one_conservative_byte_budget() {
    let mut req = request();
    req.input = "🙂".repeat(6000);
    if let Question::Predicate { instructions, .. } = &mut req.questions[0] {
        *instructions = "🙂".repeat(6000);
    }
    let (clef, transport) = provider(LIVE.into());
    assert!(clef.decide(&req).is_err());
    assert!(transport.requests.lock().unwrap().is_empty());
}

#[test]
fn clef_contract_multibyte_context_is_refused_before_remote_truncation() {
    let (clef, transport) = provider(inbound_reply());
    let gate = Gate {
        remote: Some(Box::new(clef)),
        ..Gate::default()
    };
    // Still within the existing character limit, but nearly 24 KiB before the
    // conversation questions and provider framing are added.
    let mut check = Check::new(Point::ConversationMessage, "🙂".repeat(6000));
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Ask);
    check.irreversible = true;
    assert_eq!(gate.check_with(&check, false).outcome, Outcome::Block);
    assert!(transport.requests.lock().unwrap().is_empty());
}

#[test]
fn clef_contract_large_question_uses_the_same_smaller_flash_envelope() {
    let mut req = request();
    if let Question::Predicate { instructions, .. } = &mut req.questions[0] {
        *instructions = "🙂".repeat(6000);
    }
    let (clef, transport) = provider(LIVE.into());
    assert!(clef.decide(&req).is_err());
    assert!(transport.requests.lock().unwrap().is_empty());
}
