//! Typed public locators only; semantic arguments are never classified away.
use super::{ToolCall, ToolDef};
use serde_json::Value;

/// Public canonical references minted by a trusted host after exact artifact/head
/// validation. This type cannot designate semantic payload fields as public and is
/// deliberately not deserializable from model or browser input.
#[derive(Clone, Debug)]
pub struct PublicArtifactReferences {
    canonical_tool: String,
    artifact_id: String,
    sha256: Option<String>,
    page_head: Option<(String, String)>,
}
impl PublicArtifactReferences {
    pub fn new(
        canonical_tool: String,
        artifact_id: String,
        sha256: Option<String>,
    ) -> Option<Self> {
        let token = |s: &str, max: usize| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        };
        if !token(&canonical_tool, 64)
            || !token(&artifact_id, 128)
            || sha256.as_ref().is_some_and(|sha| {
                sha.len() != 64
                    || !sha
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return None;
        }
        Some(Self {
            canonical_tool,
            artifact_id,
            sha256,
            page_head: None,
        })
    }
    /// The native host already verified both exact public Page CAS fingerprints.
    /// This constructor cannot designate arbitrary/model-selected fields as public.
    pub fn page_head(artifact_id: String, prefix: String, content: String) -> Option<Self> {
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !digest(&prefix) || !digest(&content) {
            return None;
        }
        let mut refs = Self::new("replace_page_block".into(), artifact_id, None)?;
        refs.page_head = Some((prefix, content));
        Some(refs)
    }
    pub(super) fn gate_text(&self, call: &ToolCall, definition: &ToolDef) -> Option<String> {
        let mut semantic = call.arguments.as_object()?.clone();
        let exact_sha = match (&self.sha256, semantic.get("sha256")) {
            (None, None) => true,
            (Some(expected), Some(Value::String(actual))) => expected == actual,
            _ => false,
        };
        if semantic.get("artifactId").and_then(Value::as_str) != Some(self.artifact_id.as_str())
            || !exact_sha
        {
            return None;
        }
        semantic.remove("artifactId");
        semantic.remove("sha256");
        if let Some((prefix, content)) = &self.page_head {
            if semantic.get("prefixSha256").and_then(Value::as_str) != Some(prefix)
                || semantic.get("contentSha256").and_then(Value::as_str) != Some(content)
            {
                return None;
            }
            semantic.remove("prefixSha256");
            semantic.remove("contentSha256");
        }
        Some(format!(
            "mini-app tool {} ({}) {}",
            self.canonical_tool,
            definition.description,
            Value::Object(semantic)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_public_fingerprints_are_exact_and_semantic_credential_checks_remain() {
        let prefix = "ab".repeat(32);
        let content = "cd".repeat(32);
        let refs =
            PublicArtifactReferences::page_head("it_page".into(), prefix.clone(), content.clone())
                .unwrap();
        let mut call = ToolCall {
            requires_review: false,
            id: "synthetic".into(),
            name: "replace_page_block_hash".into(),
            arguments: serde_json::json!({"artifactId":"it_page","prefixSha256":prefix,"contentSha256":content,"before":{"text":"before"},"after":{"text":"after"}}),
        };
        let def = ToolDef {
            name: call.name.clone(),
            description: "Update reviewed Page text".into(),
            parameters: serde_json::json!({}),
        };
        let text = refs.gate_text(&call, &def).unwrap();
        assert!(!text.contains(&prefix) && !text.contains(&content));
        assert!(text.contains("before") && text.contains("after"));
        assert!(gate::hard_rules(&gate::Check::new(gate::Point::ToolCall, &text)).is_none());
        call.arguments["after"]["text"] = "auth.json".into();
        let text = refs.gate_text(&call, &def).unwrap();
        assert!(matches!(
            gate::hard_rules(&gate::Check::new(gate::Point::ToolCall, &text)),
            Some((gate::Outcome::Block, _))
        ));
        call.arguments["prefixSha256"] = "ef".repeat(32).into();
        assert!(refs.gate_text(&call, &def).is_none());
        assert!(PublicArtifactReferences::page_head(
            "it_page".into(),
            "not-a-digest".into(),
            content
        )
        .is_none());
    }
}
