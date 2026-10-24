//! Fixed format dispatch over the existing binary document/approval contract.
use super::Format;
use crate::deckcraft;
#[path = "../gridcraft/agent_tools.rs"]
mod gridcraft_tools;
pub use deckcraft::agent_tools::{canonical_json, Definition, ParsedCommand};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn format(tool: &str) -> Option<Format> {
    if deckcraft::agent_tools::recognizes(tool) {
        Some(Format::Deckcraft)
    } else if gridcraft_tools::recognizes(tool) {
        Some(Format::Gridcraft)
    } else {
        None
    }
}
pub fn tools_for(kind: Format) -> Vec<Definition> {
    match kind {
        Format::Deckcraft => deckcraft::agent_tools::tools(),
        Format::Gridcraft => gridcraft_tools::tools(),
    }
}
pub fn tools() -> Vec<Definition> {
    let mut out = tools_for(Format::Deckcraft);
    out.extend(tools_for(Format::Gridcraft));
    out
}
pub fn command(tool: &str, args: &Value) -> Result<ParsedCommand, String> {
    match format(tool) {
        Some(Format::Deckcraft) => deckcraft::agent_tools::command(tool, args),
        Some(Format::Gridcraft) => gridcraft_tools::command(tool, args),
        None => Err("Unsupported binary App tool".into()),
    }
}
pub fn recognizes(tool: &str) -> bool {
    format(tool).is_some()
}
pub fn is_read(tool: &str) -> bool {
    matches!(tool, "read_deckcraft" | "read_gridcraft")
}
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    if format(tool) == Some(Format::Deckcraft) {
        return deckcraft::agent_tools::model_name(tool, item);
    }
    let digest = format!("{:x}", Sha256::digest(item.as_bytes()));
    let name = format!("{tool}__{}", &digest[..40]);
    (format(tool) == Some(Format::Gridcraft)
        && !item.is_empty()
        && item.len() <= 128
        && item
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && name.len() <= 64)
        .then_some(name)
}
pub fn is_model_name(name: &str) -> bool {
    name.rsplit_once("__").is_some_and(|(tool, digest)| {
        recognizes(tool)
            && digest.len() == 40
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[cfg(test)]
mod tests;
