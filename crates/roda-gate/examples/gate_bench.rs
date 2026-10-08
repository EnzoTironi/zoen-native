//! Red-team bench: runs fixtures/redteam.json through each System One server you list.
//!
//!   GATE_PROVIDERS="kev-0.8b=http://127.0.0.1:8009#kev-latest,laya-typed=http://127.0.0.1:8010#typed-decisions" \
//!   cargo run -p roda-gate --example gate_bench
//!
//! Bearer tokens (Jev, Laya Studio) come from GATE_BEARER_<NAME> in a dev .env; never from
//! the repo. Reports, per provider: model alone and gate (hard rules + model), with the
//! default thresholds and the "escalate to ask" ones.

use roda_gate::*;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

struct Curl;
impl HttpTransport for Curl {
    fn post_json(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<String, String> {
        let mut cmd = Command::new("curl");
        cmd.args([
            "-sS",
            "--max-time",
            "60",
            "-H",
            "content-type: application/json",
            "-X",
            "POST",
            url,
            "--data-binary",
            "@-",
        ]);
        if let Some(b) = bearer {
            cmd.args(["-H", &format!("authorization: Bearer {b}")]);
        }
        use std::io::Write;
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .map_err(|e| e.to_string())?;
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into())
    }
}

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    point: String,
    text: String,
    intent: Option<String>,
    expect: String,
}

fn check(c: &Case) -> Check {
    let point = match c.point.as_str() {
        "tool-call" => Point::ToolCall,
        "inbound" => Point::Inbound,
        "bundle" => Point::Bundle,
        _ => Point::Outbound,
    };
    let mut k = Check::new(point, c.text.clone());
    k.user_intent = c.intent.clone();
    k
}

#[derive(Default)]
struct Score {
    tp: u32,
    fp: u32,
    fn_: u32,
    tn: u32,
    exact: u32,
    n: u32,
    lat: Vec<u64>,
    errors: u32,
    risk: Vec<(f64, bool)>,
}
impl Score {
    fn add(&mut self, expect: &str, d: &Decision) {
        let unsafe_ = expect != "allow";
        let flagged = d.outcome != Outcome::Allow;
        match (unsafe_, flagged) {
            (true, true) => self.tp += 1,
            (false, true) => self.fp += 1,
            (true, false) => self.fn_ += 1,
            (false, false) => self.tn += 1,
        }
        if d.outcome.id() == expect {
            self.exact += 1;
        }
        if d.rule.starts_with("fail:") {
            self.errors += 1;
        }
        if d.provider != "rules" {
            self.lat.push(d.latency_ms);
        }
        if !d.answers.is_empty() {
            self.risk.push((risk(&d.answers), unsafe_));
        }
        self.n += 1;
    }
    fn row(&mut self, name: &str) -> String {
        let p = if self.tp + self.fp == 0 {
            0.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        };
        let r = if self.tp + self.fn_ == 0 {
            0.0
        } else {
            self.tp as f64 / (self.tp + self.fn_) as f64
        };
        self.lat.sort();
        let pct = |q: f64| {
            self.lat
                .get(((self.lat.len() as f64 - 1.0) * q).round() as usize)
                .copied()
                .unwrap_or(0)
        };
        format!(
            "| {name} | {:.2} | {:.2} | {}/{} | {}/{}/{}/{} | {} | {} | {} ms | {} ms |",
            p,
            r,
            self.exact,
            self.n,
            self.tp,
            self.fp,
            self.fn_,
            self.tn,
            self.errors,
            auroc(&self.risk)
                .map(|a| format!("{a:.2}"))
                .unwrap_or("–".into()),
            pct(0.5),
            pct(0.95)
        )
    }
}

/// One risk number per decision, from the model's raw answers: the most alarming signal.
/// (`matches_user_intent` counts inverted; 4-level scores are divided by 3; choices ignored.)
fn risk(a: &std::collections::BTreeMap<String, f64>) -> f64 {
    a.iter()
        .filter(|(k, _)| k.as_str() != "policy_category")
        .map(|(k, v)| match k.as_str() {
            "matches_user_intent" => 1.0 - v,
            "severity" | "harmful_content" => v / 3.0,
            _ => *v,
        })
        .fold(0.0, f64::max)
}

/// Area under the ROC curve: chance a random unsafe case scores riskier than a random safe one.
fn auroc(xs: &[(f64, bool)]) -> Option<f64> {
    let pos: Vec<f64> = xs.iter().filter(|x| x.1).map(|x| x.0).collect();
    let neg: Vec<f64> = xs.iter().filter(|x| !x.1).map(|x| x.0).collect();
    if pos.is_empty() || neg.is_empty() {
        return None;
    }
    let mut wins = 0.0;
    for p in &pos {
        for n in &neg {
            wins += if p > n {
                1.0
            } else if p == n {
                0.5
            } else {
                0.0
            };
        }
    }
    Some(wins / (pos.len() * neg.len()) as f64)
}

fn main() {
    // GATE_FIXTURES=heldout runs the held-out set (never used for calibrating thresholds).
    let raw = if std::env::var("GATE_FIXTURES").as_deref() == Ok("heldout") {
        include_str!("../fixtures/heldout.json")
    } else {
        include_str!("../fixtures/redteam.json")
    };
    let cases: Vec<Case> = serde_json::from_str(raw).unwrap();
    let spec = std::env::var("GATE_PROVIDERS").unwrap_or_default();
    type Factory = Box<dyn Fn() -> Box<dyn DecisionProvider>>;
    let mut providers: Vec<(String, Factory)> = vec![(
        "mock".into(),
        Box::new(|| Box::new(Mock) as Box<dyn DecisionProvider>),
    )];
    for entry in spec.split(',').filter(|s| !s.is_empty()) {
        let (name, rest) = entry.split_once('=').expect("name=url#model");
        let (url, model) = rest
            .split_once('#')
            .map(|(u, m)| (u.to_string(), Some(m.to_string())))
            .unwrap_or((rest.to_string(), None));
        let bearer = std::env::var(format!(
            "GATE_BEARER_{}",
            name.to_uppercase().replace(['-', '.'], "_")
        ))
        .ok();
        let (n, u, m, b) = (name.to_string(), url, model, bearer);
        providers.push((
            n.clone(),
            Box::new(move || {
                Box::new(SystemOneHttp {
                    transport: Arc::new(Curl),
                    base_url: u.clone(),
                    bearer: b.clone(),
                    model: m.clone(),
                    label: n.clone(),
                }) as Box<dyn DecisionProvider>
            }),
        ));
    }
    println!("| provider · mode | precision | recall | exact | tp/fp/fn/tn | fail | AUROC | p50 | p95 |\n|---|---|---|---|---|---|---|---|---|");
    let mut detail = String::new();
    for (name, make) in &providers {
        let mut modes: Vec<(&str, Thresholds)> =
            if name == "mock" || std::env::var("GATE_ESCALATE").is_ok() {
                vec![
                    ("default", Thresholds::default()),
                    ("escalate", Thresholds::escalate_to_ask()),
                ]
            } else {
                vec![("default", Thresholds::default())]
            };
        if name != "mock" {
            modes.push(("laya-preset", Thresholds::laya_local()));
        }
        for (tname, th) in modes {
            for rules in [false, true] {
                let gate = Gate {
                    remote: Some(make()),
                    on_device: None,
                    fallback: Mock,
                    thresholds: th.clone(),
                };
                let mut s = Score::default();
                let t0 = Instant::now();
                for c in &cases {
                    let d = gate.check_with(&check(c), rules);
                    if rules && tname == "default" {
                        detail.push_str(&format!(
                            "{name:<12} {:<26} expect {:<5} got {:<5} {} {:?}\n",
                            c.id,
                            c.expect,
                            d.outcome.id(),
                            d.rule,
                            d.answers
                        ));
                    }
                    s.add(&c.expect, &d);
                }
                let _ = t0;
                println!(
                    "{}",
                    s.row(&format!(
                        "{name} · {} · {tname}",
                        if rules { "rules+model" } else { "model only" }
                    ))
                );
            }
        }
    }
    println!("\n{detail}");
}
