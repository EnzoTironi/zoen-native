use super::{system_one_body, HttpTransport, SystemOneHttp};
use crate::{DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use serde_json::Value;
use std::sync::Arc;

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
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            ClefModel::Clef => "clef",
            ClefModel::Flash => "clef-flash",
        }
    }
}

impl Clef {
    pub const API: &'static str = "https://api.cloudflare.com/client/v4";

    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "{}/accounts/{}/ai/run/@cf/cloudflare/{}",
            Self::API,
            self.account,
            self.model.id()
        )
    }

    /// # Errors
    /// Returns an error when the response is malformed or violates the provider contract.
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
    if (values.iter().sum::<f64>() - 1.0).abs() > clef_count(keys.len())? * 0.00005 + 1e-8 {
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
            let score = clef_number(answer.get("score"), clef_count(levels.len() - 1)?)?;
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
                .zip(0_u32..)
                .map(|(p, index)| f64::from(index) * p)
                .sum::<f64>();
            let rounding = clef_count(levels.len() * (levels.len() - 1) / 2 + 1)? * 0.00005 + 1e-8;
            if (score - expected).abs() > rounding {
                return Err(clef_invalid("score disagrees with probabilities"));
            }
        }
    }
    Ok(())
}

fn clef_count(count: usize) -> Result<f64, GateError> {
    u32::try_from(count)
        .map(f64::from)
        .map_err(|_| clef_invalid("count exceeds numeric representation"))
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maximum_choice_and_score_bounds_preserve_probability_contract() -> Result<(), GateError> {
        let choices: Vec<_> = (0..255)
            .map(|i| (format!("c{i}"), format!("Choice {i}")))
            .collect();
        let levels: Vec<_> = (0..10).map(|i| format!("Level {i}")).collect();
        let req = DecisionRequest {
            input: "Synthetic bounds fixture".into(),
            questions: vec![
                Question::Choice {
                    name: "choice".into(),
                    instructions: "Choose".into(),
                    choices: choices.clone(),
                },
                Question::Score {
                    name: "score".into(),
                    instructions: "Score".into(),
                    levels: levels.clone(),
                },
            ],
        };
        let probabilities: serde_json::Map<_, _> = choices
            .iter()
            .enumerate()
            .map(|(i, (key, _))| (key.clone(), json!(if i == 254 { 1.0 } else { 0.0 })))
            .collect();
        let score_probabilities: serde_json::Map<_, _> =
            (0..10).map(|i| (i.to_string(), json!(0.1))).collect();
        let legend: serde_json::Map<_, _> = levels
            .iter()
            .enumerate()
            .map(|(i, level)| (i.to_string(), json!(level)))
            .collect();
        let mut reply = json!({"success":true,"errors":[],"result":{
            "model":"clef","usage":{"input_tokens":1,"output_tokens":1},"answers":{
                "choice":{"type":"choice","choice":"c254","confidence":1.0,"probabilities":probabilities},
                "score":{"type":"score","score":4.5022,"confidence":1.0,"probabilities":score_probabilities,"legend":legend}
            }
        }});
        let decoded = Clef::parse(&req, &reply.to_string())?;
        assert_eq!(
            decoded.answers["choice"],
            crate::Answer::Choice {
                choice: "c254".into(),
                confidence: 1.0
            }
        );
        assert_eq!(
            decoded.answers["score"],
            crate::Answer::Score {
                score: 4.5022,
                confidence: 1.0
            }
        );
        reply["result"]["answers"]["score"]["score"] = json!(4.5024);
        assert!(Clef::parse(&req, &reply.to_string()).is_err());
        reply["result"]["answers"]["score"]["score"] = json!(9.0001);
        assert!(Clef::parse(&req, &reply.to_string()).is_err());
        Ok(())
    }

    #[test]
    fn counts_convert_exactly_or_refuse() -> Result<(), GateError> {
        assert_eq!(clef_count(255)?, 255.0);
        assert_eq!(clef_count(10)?, 10.0);
        assert_eq!(clef_count(46)?, 46.0);
        if let Ok(overflow) = usize::try_from(u64::from(u32::MAX) + 1) {
            assert!(clef_count(overflow).is_err());
        }
        Ok(())
    }
}
