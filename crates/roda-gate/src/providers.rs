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

impl SystemOneHttp {
    pub fn body(&self, req: &DecisionRequest) -> Value {
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
        if let Some(m) = &self.model {
            b["model"] = json!(m);
        }
        b
    }

    pub fn parse(req: &DecisionRequest, body: &str) -> Result<DecisionResponse, GateError> {
        let v: Value = serde_json::from_str(body).map_err(|e| {
            GateError::BadResponse(format!(
                "{e}: {}",
                body.chars().take(200).collect::<String>()
            ))
        })?;
        let answers = v
            .get("answers")
            .and_then(|a| a.as_object())
            .ok_or_else(|| {
                GateError::BadResponse(format!(
                    "no answers: {}",
                    body.chars().take(200).collect::<String>()
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
        let shape = SystemOneHttp {
            transport: Arc::new(NoTransport),
            base_url: String::new(),
            bearer: None,
            model: None,
            label: String::new(),
        };
        let out = self
            .judge
            .decide_json(shape.body(req).to_string())
            .ok_or_else(|| GateError::Unavailable("on-device model unavailable".into()))?;
        SystemOneHttp::parse(req, &out)
    }
}

struct NoTransport;
impl HttpTransport for NoTransport {
    fn post_json(&self, _: &str, _: Option<&str>, _: &str) -> Result<String, String> {
        Err("no transport".into())
    }
}
