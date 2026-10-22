//! Providers. HTTP goes through a host-supplied [`HttpTransport`] so the core never links an
//! HTTP stack or holds a key: on iOS the transport will point at Zoen's server-side proxy;
//! the box's bench uses curl with a key from a dev `.env`.

use crate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub trait HttpTransport: Send + Sync {
    /// POST `body` (JSON) to `url`; returns the response body or an error string.
    fn post_json(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<String, String>;
}

// ── System One (Jev, Laya, Kev) ──

/// `POST {base_url}/v1/systemone`. Jev in production (`https://api.typesafe.ai`), Laya or
/// Kev locally (`http://127.0.0.1:8000`): the same request and response, so swapping is
/// config only.
pub struct SystemOneHttp {
    pub transport: Arc<dyn HttpTransport>,
    pub base_url: String,
    pub bearer: Option<String>,
    /// Sent as `model`; Laya routes on `typed-decisions`/`english`/`multilingual`, Jev on
    /// its model id. `None` lets the server route.
    pub model: Option<String>,
    pub label: String,
}

/// The System One request: `state` plus named typed questions.
fn system_one_body(req: &DecisionRequest, model: Option<&str>) -> Value {
    let mut qs = Map::new();
    for q in &req.questions {
        let v = match q {
            Question::Predicate { instructions, .. } => {
                json!({ "type": "noul", "instructions": instructions })
            }
            Question::Choice {
                instructions,
                choices,
                ..
            } => {
                let crit: Map<String, Value> =
                    choices.iter().map(|(k, d)| (k.clone(), json!(d))).collect();
                json!({ "type": "choice", "instructions": instructions, "criteria": crit })
            }
            Question::Score {
                instructions,
                levels,
                ..
            } => json!({ "type": "score", "instructions": instructions, "criteria": levels }),
        };
        qs.insert(q.name().to_string(), v);
    }
    let mut b = json!({ "state": req.input, "questions": qs });
    if let Some(m) = model {
        b["model"] = json!(m);
    }
    b
}

impl SystemOneHttp {
    pub fn body(&self, req: &DecisionRequest) -> Value {
        system_one_body(req, self.model.as_deref())
    }

    pub fn parse(req: &DecisionRequest, body: &str) -> Result<DecisionResponse, GateError> {
        let v: Value = serde_json::from_str(body).map_err(|e| {
            GateError::BadResponse(format!(
                "{e}: {}",
                body.chars().take(200).collect::<String>()
            ))
        })?;
        Self::parse_value(req, &v)
    }

    fn parse_value(req: &DecisionRequest, v: &Value) -> Result<DecisionResponse, GateError> {
        let answers = v
            .get("answers")
            .and_then(|a| a.as_object())
            .ok_or_else(|| {
                GateError::BadResponse(format!(
                    "no answers: {}",
                    v.to_string().chars().take(200).collect::<String>()
                ))
            })?;
        let mut out = DecisionResponse::default();
        for q in &req.questions {
            let Some(a) = answers.get(q.name()) else {
                continue;
            };
            if a.get("refusal").is_some()
                || a.get("type").and_then(|t| t.as_str()) == Some("refusal")
            {
                out.answers.insert(q.name().into(), Answer::Refusal);
                continue;
            }
            let conf = a.get("confidence").and_then(|c| c.as_f64()).unwrap_or(1.0);
            let ans = match q {
                Question::Predicate { .. } => a
                    .get("noul")
                    .and_then(|p| p.as_f64())
                    .map(|p| Answer::Predicate { p }),
                Question::Choice { .. } => {
                    a.get("choice")
                        .and_then(|c| c.as_str())
                        .map(|c| Answer::Choice {
                            choice: c.into(),
                            confidence: conf,
                        })
                }
                Question::Score { .. } => {
                    a.get("score")
                        .and_then(|s| s.as_f64())
                        .map(|s| Answer::Score {
                            score: s,
                            confidence: conf,
                        })
                }
            };
            if let Some(ans) = ans {
                out.answers.insert(q.name().into(), ans);
            }
        }
        Ok(out)
    }
}

impl DecisionProvider for SystemOneHttp {
    fn name(&self) -> String {
        self.label.clone()
    }
    fn on_device(&self) -> bool {
        false
    }
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let url = format!("{}/v1/systemone", self.base_url.trim_end_matches('/'));
        let body = self
            .transport
            .post_json(&url, self.bearer.as_deref(), &self.body(req).to_string())
            .map_err(GateError::Transport)?;
        Self::parse(req, &body)
    }
}

// ── Cloudflare Clef ──

// Text-only Gate profile: bound the complete serialized state + questions.
// Clef-flash documents 24,576 context tokens; use a smaller 16-KiB application
// envelope to leave framing headroom, rather than relying on remote truncation.
// This byte budget is not a tokenizer or an upstream maximum.
const MAX_CLEF_REQUEST_BYTES: usize = 16 * 1024;

/// Cloudflare's Clef decision models on Workers AI (`@cf/cloudflare/clef`, `clef-flash`).
/// System One's request and answers, inside Cloudflare's `{ success, result, errors }`
/// envelope at `/accounts/{account}/ai/run/@cf/cloudflare/{model}`.
pub struct Clef {
    pub transport: Arc<dyn HttpTransport>,
    pub account: String,
    pub token: String,
    pub model: ClefModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClefModel {
    Clef,
    Flash,
}

impl ClefModel {
    pub fn id(self) -> &'static str {
        match self {
            ClefModel::Clef => "clef",
            ClefModel::Flash => "clef-flash",
        }
    }
}

impl Clef {
    pub const API: &'static str = "https://api.cloudflare.com/client/v4";

    pub fn url(&self) -> String {
        format!(
            "{}/accounts/{}/ai/run/@cf/cloudflare/{}",
            Self::API,
            self.account,
            self.model.id()
        )
    }

    pub fn parse(req: &DecisionRequest, body: &str) -> Result<DecisionResponse, GateError> {
        Self::parse_for_model(req, body, None)
    }

    fn parse_for_model(
        req: &DecisionRequest,
        body: &str,
        model: Option<ClefModel>,
    ) -> Result<DecisionResponse, GateError> {
        validate_clef_request(req)?;
        if body.len() > 1024 * 1024 {
            return Err(clef_invalid("response too large"));
        }
        let v: Value =
            serde_json::from_str(body).map_err(|e| GateError::BadResponse(e.to_string()))?;
        if v.get("success").and_then(Value::as_bool) != Some(true)
            || v.get("errors")
                .and_then(Value::as_array)
                .is_none_or(|errors| !errors.is_empty())
        {
            return Err(GateError::Unavailable("Cloudflare decision refused".into()));
        }
        let result = v
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| clef_invalid("missing result"))?;
        let reported = result
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| clef_invalid("missing model"))?;
        if !["clef", "clef-flash"].contains(&reported)
            || model.is_some_and(|model| model.id() != reported)
        {
            return Err(clef_invalid("model mismatch"));
        }
        let usage = result
            .get("usage")
            .and_then(Value::as_object)
            .ok_or_else(|| clef_invalid("missing usage"))?;
        if ["input_tokens", "output_tokens"]
            .iter()
            .any(|key| usage.get(*key).and_then(Value::as_u64).is_none())
        {
            return Err(clef_invalid("invalid usage"));
        }
        let answers = result
            .get("answers")
            .and_then(Value::as_object)
            .ok_or_else(|| clef_invalid("missing answers"))?;
        if answers.len() != req.questions.len() {
            return Err(clef_invalid("answer keys mismatch"));
        }
        for question in &req.questions {
            let answer = answers
                .get(question.name())
                .ok_or_else(|| clef_invalid("answer keys mismatch"))?;
            validate_clef_answer(question, answer)?;
        }
        // Keep the existing System One decoder as the sole owned answer mapping.
        SystemOneHttp::parse_value(req, &v["result"])
    }
}

impl DecisionProvider for Clef {
    fn name(&self) -> String {
        format!("cloudflare:{}", self.model.id())
    }
    fn on_device(&self) -> bool {
        false
    }
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        validate_clef_request(req)?;
        let body = system_one_body(req, Some(self.model.id())).to_string();
        if body.len() > MAX_CLEF_REQUEST_BYTES {
            return Err(clef_invalid("request too large"));
        }
        let out = self
            .transport
            .post_json(&self.url(), Some(&self.token), &body)
            .map_err(GateError::Transport)?;
        Self::parse_for_model(req, &out, Some(self.model))
    }
}

fn clef_invalid(reason: &str) -> GateError {
    GateError::BadResponse(format!("Clef: {reason}"))
}

fn validate_clef_request(req: &DecisionRequest) -> Result<(), GateError> {
    let text =
        |s: &str| !s.trim().is_empty() && s.chars().count() <= crate::MAX_DECISION_INPUT_CHARS;
    let mut names = std::collections::BTreeSet::new();
    if !text(&req.input) || !(1..=64).contains(&req.questions.len()) {
        return Err(clef_invalid("invalid request bounds"));
    }
    for question in &req.questions {
        let name = question.name();
        if name.is_empty()
            || name.len() > 100
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || !names.insert(name)
        {
            return Err(clef_invalid("invalid question keys"));
        }
        let valid = match question {
            Question::Predicate { instructions, .. } => text(instructions),
            Question::Choice {
                instructions,
                choices,
                ..
            } => {
                let keys: std::collections::BTreeSet<_> =
                    choices.iter().map(|(key, _)| key).collect();
                text(instructions)
                    && (2..=255).contains(&choices.len())
                    && keys.len() == choices.len()
                    && choices.iter().all(|(key, description)| {
                        text(key) && description.chars().count() <= crate::MAX_DECISION_INPUT_CHARS
                    })
            }
            Question::Score {
                instructions,
                levels,
                ..
            } => {
                text(instructions)
                    && (2..=10).contains(&levels.len())
                    && levels.iter().all(|level| text(level))
            }
        };
        if !valid {
            return Err(clef_invalid("invalid question bounds"));
        }
    }
    Ok(())
}

fn clef_number(value: Option<&Value>, max: f64) -> Result<f64, GateError> {
    value
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && (0.0..=max).contains(n))
        .ok_or_else(|| clef_invalid("invalid probability or score"))
}

fn clef_probabilities(answer: &Value, keys: &[&str]) -> Result<Vec<f64>, GateError> {
    let probabilities = answer
        .get("probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| clef_invalid("missing probabilities"))?;
    if probabilities.len() != keys.len() {
        return Err(clef_invalid("probability keys mismatch"));
    }
    let values = keys
        .iter()
        .map(|key| clef_number(probabilities.get(*key), 1.0))
        .collect::<Result<Vec<_>, _>>()?;
    // Hosted Clef rounds probabilities to four decimals. Permit only the
    // aggregate rounding error of those reported values, not renormalization.
    if (values.iter().sum::<f64>() - 1.0).abs() > keys.len() as f64 * 0.00005 + 1e-8 {
        return Err(clef_invalid("probabilities do not sum to one"));
    }
    Ok(values)
}

fn validate_clef_answer(question: &Question, answer: &Value) -> Result<(), GateError> {
    let expected_type = match question {
        Question::Predicate { .. } => "noul",
        Question::Choice { .. } => "choice",
        Question::Score { .. } => "score",
    };
    if answer.get("type").and_then(Value::as_str) != Some(expected_type) {
        return Err(clef_invalid("answer type mismatch"));
    }
    match question {
        Question::Predicate { .. } => {
            clef_number(answer.get("noul"), 1.0)?;
        }
        Question::Choice { choices, .. } => {
            clef_number(answer.get("confidence"), 1.0)?;
            let keys: Vec<_> = choices.iter().map(|(key, _)| key.as_str()).collect();
            let probabilities = clef_probabilities(answer, &keys)?;
            let choice = answer
                .get("choice")
                .and_then(Value::as_str)
                .and_then(|choice| keys.iter().position(|key| *key == choice))
                .ok_or_else(|| clef_invalid("choice is not offered"))?;
            if probabilities
                .iter()
                .any(|p| *p > probabilities[choice] + 0.0001)
            {
                return Err(clef_invalid("choice is not highest probability"));
            }
        }
        Question::Score { levels, .. } => {
            clef_number(answer.get("confidence"), 1.0)?;
            let score = clef_number(answer.get("score"), (levels.len() - 1) as f64)?;
            let names: Vec<_> = (0..levels.len()).map(|index| index.to_string()).collect();
            let keys: Vec<_> = names.iter().map(String::as_str).collect();
            let probabilities = clef_probabilities(answer, &keys)?;
            let legend = answer
                .get("legend")
                .and_then(Value::as_object)
                .ok_or_else(|| clef_invalid("missing legend"))?;
            if legend.len() != levels.len()
                || levels.iter().enumerate().any(|(index, level)| {
                    legend.get(&names[index]).and_then(Value::as_str) != Some(level)
                })
            {
                return Err(clef_invalid("legend mismatch"));
            }
            let expected = probabilities
                .iter()
                .enumerate()
                .map(|(index, p)| index as f64 * p)
                .sum::<f64>();
            let rounding = ((levels.len() * (levels.len() - 1) / 2 + 1) as f64) * 0.00005 + 1e-8;
            if (score - expected).abs() > rounding {
                return Err(clef_invalid("score disagrees with probabilities"));
            }
        }
    }
    Ok(())
}

// ── OpenAI Decisions ──

/// `POST {base_url}/decisions` with `gpt-6-luna` (public beta). Optional backend.
pub struct OpenAIDecisions {
    pub transport: Arc<dyn HttpTransport>,
    /// `https://api.openai.com/v1` or the Zoen proxy.
    pub base_url: String,
    pub bearer: Option<String>,
    pub model: String,
}

impl OpenAIDecisions {
    pub fn body(&self, req: &DecisionRequest) -> Value {
        let qs: Vec<Value> = req.questions.iter().map(|q| match q {
            Question::Predicate { name, instructions } => json!({ "type": "predicate", "name": name, "instructions": instructions }),
            Question::Choice { name, instructions, choices } => json!({ "type": "choice", "name": name, "instructions": instructions,
                "choices": choices.iter().map(|(v, d)| json!({ "value": v, "description": d })).collect::<Vec<_>>() }),
            Question::Score { name, instructions, levels } => json!({ "type": "score", "name": name, "instructions": instructions,
                "levels": levels.iter().map(|l| json!({ "label": l })).collect::<Vec<_>>() }),
        }).collect();
        json!({ "model": self.model, "input": req.input, "questions": qs })
    }

    pub fn parse(body: &str) -> Result<DecisionResponse, GateError> {
        let v: Value =
            serde_json::from_str(body).map_err(|e| GateError::BadResponse(e.to_string()))?;
        let arr = v
            .get("answers")
            .and_then(|a| a.as_array())
            .ok_or_else(|| GateError::BadResponse(body.chars().take(200).collect()))?;
        let mut out = DecisionResponse::default();
        for a in arr {
            let name = a
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let conf = a.get("confidence").and_then(|c| c.as_f64()).unwrap_or(1.0);
            let ans = match a.get("type").and_then(|t| t.as_str()) {
                Some("predicate") => a
                    .get("probability")
                    .and_then(|p| p.as_f64())
                    .map(|p| Answer::Predicate { p }),
                Some("choice") => {
                    a.get("choice")
                        .and_then(|c| c.as_str())
                        .map(|c| Answer::Choice {
                            choice: c.into(),
                            confidence: conf,
                        })
                }
                Some("score") => a
                    .get("score")
                    .and_then(|s| s.as_f64())
                    .map(|s| Answer::Score {
                        score: s,
                        confidence: conf,
                    }),
                Some("refusal") => Some(Answer::Refusal),
                _ => None,
            };
            if let Some(ans) = ans {
                out.answers.insert(name, ans);
            }
        }
        Ok(out)
    }
}

impl DecisionProvider for OpenAIDecisions {
    fn name(&self) -> String {
        format!("openai:{}", self.model)
    }
    fn on_device(&self) -> bool {
        false
    }
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        let url = format!("{}/decisions", self.base_url.trim_end_matches('/'));
        let body = self
            .transport
            .post_json(&url, self.bearer.as_deref(), &self.body(req).to_string())
            .map_err(GateError::Transport)?;
        Self::parse(&body)
    }
}

// ── On device (Foundation Models through the app) ──

/// Implemented by the app (Swift, Apple Foundation Models). Gets the System One request
/// JSON and returns System One response JSON, or `None` when the model isn't available.
pub trait OnDeviceJudge: Send + Sync {
    fn decide_json(&self, request_json: String) -> Option<String>;
}

pub struct OnDevice {
    pub judge: Arc<dyn OnDeviceJudge>,
}

impl DecisionProvider for OnDevice {
    fn name(&self) -> String {
        "on-device".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        // Same wire shape as System One, so the Swift side has one format to answer.
        let out = self
            .judge
            .decide_json(system_one_body(req, None).to_string())
            .ok_or_else(|| GateError::Unavailable("on-device model unavailable".into()))?;
        SystemOneHttp::parse(req, &out)
    }
}
