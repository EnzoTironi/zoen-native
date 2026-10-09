//! The agent's browser tools (T2, ADR 0028 §7): what a model can do with a browser microVM.
//!
//! Four tools, nothing else: open a URL, read the page as text, click an element, type into
//! an element. There is no screenshot, no script evaluation and no shell for a browser lease,
//! so the model sees pages only through [`BrowserCall::Read`], where password and
//! one-time-code fields read as `[hidden]` and anything the owner typed during a takeover is
//! scrubbed. Typing into those fields is refused: the owner does it in a takeover, from their
//! device, end-to-end encrypted to the VM. During a takeover every call here is refused.

use crate::sandbox::{Lease, SandboxError};
use async_trait::async_trait;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserCall {
    Open {
        url: String,
    },
    Read,
    /// `target`: an element number from `Read` (`"12"`) or a CSS selector.
    Click {
        target: String,
    },
    Type {
        target: String,
        text: String,
        submit: bool,
    },
}

pub const TOOL_NAMES: [&str; 4] = [
    "browser_open",
    "browser_read",
    "browser_click",
    "browser_type",
];

/// The tool definitions handed to the model (JSON Schema parameters).
pub fn tool_definitions() -> Value {
    json!([
        {"name": "browser_open", "description": "Open a web page (http or https) in your browser.",
         "parameters": {"type": "object", "required": ["url"],
                        "properties": {"url": {"type": "string"}}}},
        {"name": "browser_read", "description": "Read the current page as text, with its links, buttons and fields numbered.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_click", "description": "Click an element: its number from browser_read, or a CSS selector.",
         "parameters": {"type": "object", "required": ["target"],
                        "properties": {"target": {"type": "string"}}}},
        {"name": "browser_type", "description": "Type into a field (not passwords or codes: ask the owner to take over for those). submit presses Enter.",
         "parameters": {"type": "object", "required": ["target", "text"],
                        "properties": {"target": {"type": "string"}, "text": {"type": "string"},
                                       "submit": {"type": "boolean"}}}}
    ])
}

impl BrowserCall {
    /// A model's tool call, by name and JSON arguments.
    pub fn parse(name: &str, args: &Value) -> Result<Self, String> {
        let s = |k: &str| {
            args.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("{name}: missing `{k}`"))
        };
        match name {
            "browser_open" => Ok(BrowserCall::Open { url: s("url")? }),
            "browser_read" => Ok(BrowserCall::Read),
            "browser_click" => Ok(BrowserCall::Click {
                target: s("target")?,
            }),
            "browser_type" => Ok(BrowserCall::Type {
                target: s("target")?,
                text: s("text")?,
                submit: args.get("submit").and_then(Value::as_bool).unwrap_or(false),
            }),
            other => Err(format!("unknown browser tool `{other}`")),
        }
    }
}

/// A backend that runs browser leases. Answers are plain text for the model.
#[async_trait]
pub trait BrowserTools: Send + Sync {
    async fn browser(&self, lease: &Lease, call: BrowserCall) -> Result<String, SandboxError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_calls_parse() {
        assert_eq!(
            BrowserCall::parse("browser_type", &json!({"target": "3", "text": "rust"})).unwrap(),
            BrowserCall::Type {
                target: "3".into(),
                text: "rust".into(),
                submit: false
            }
        );
        assert!(BrowserCall::parse("browser_eval", &json!({})).is_err());
        assert!(BrowserCall::parse("browser_open", &json!({})).is_err());
        let defs = tool_definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, TOOL_NAMES);
    }
}
