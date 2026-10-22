//! Native DeckCraft container validation for durable opaque storage, not an editor.
//! The pinned upstream Session owns model interpretation/commands/export. Never normalize
//! or repair uploaded bytes here: all versions retain exactly what their hash identifies.
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};

pub mod agent_tools;
pub mod registration;

pub const APP: &str = "deckcraft";
pub const RESOURCE: &str = "ui://zoen/deckcraft/index.html";
pub const MIME: &str = "application/vnd.storyteller.deckcraft+zip";
pub const ENGINE_REVISION: &str = "a31b990d32f2c8562f1588562f922960c3fde48b";
pub const MAX_BYTES: usize = 8 << 20;
const MAX_INFLATED_BYTES: u64 = 16 << 20;
pub struct Document {
    pub slides: u32,
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err("Invalid DeckCraft byte length");
        }
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Invalid DeckCraft container")?;
        if archive.len() > 512 {
            return Err("Too many DeckCraft entries");
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        let mut manifest = None;
        let mut mime = false;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|_| "Invalid DeckCraft entry")?;
            let name = entry.name().to_string();
            if name.is_empty()
                || name.len() > 256
                || name.starts_with('/')
                || name.contains('\\')
                || name.contains('\0')
                || name.split('/').any(|part| part == "..")
                || !names.insert(name.clone())
                || entry.size() > MAX_BYTES as u64
            {
                return Err("Invalid DeckCraft entry");
            }
            let declared_size = entry.size();
            let declared_total = total
                .checked_add(declared_size)
                .ok_or("Invalid DeckCraft size")?;
            if declared_total > MAX_INFLATED_BYTES {
                return Err("Oversized DeckCraft container");
            }
            let cap = if name == "presentation.json" {
                4 << 20
            } else {
                MAX_BYTES
            };
            if entry.size() > cap as u64 {
                return Err("Oversized DeckCraft entry");
            }
            let mut data = Vec::new();
            entry
                .by_ref()
                .take((cap as u64).min(MAX_INFLATED_BYTES - total) + 1)
                .read_to_end(&mut data)
                .map_err(|_| "Invalid DeckCraft checksum")?;
            if data.len() > cap {
                return Err("Oversized DeckCraft entry");
            }
            total = total
                .checked_add(data.len() as u64)
                .ok_or("Invalid DeckCraft size")?;
            if total > MAX_INFLATED_BYTES {
                return Err("Oversized DeckCraft container");
            }
            if data.len() as u64 != declared_size {
                return Err("Invalid DeckCraft entry size");
            }
            if name == "mimetype" {
                mime = data == MIME.as_bytes();
            }
            if name == "presentation.json" {
                manifest = Some(
                    serde_json::from_slice::<Value>(&data).map_err(|_| "Invalid DeckCraft JSON")?,
                );
            }
        }
        let manifest = manifest.ok_or("Missing DeckCraft document")?;
        let model = manifest
            .get("presentation")
            .filter(|v| v.is_object())
            .ok_or("Missing DeckCraft model")?;
        let slides = model
            .get("slides")
            .and_then(Value::as_array)
            .ok_or("Missing DeckCraft slides")?;
        if !mime
            || manifest.get("format").and_then(Value::as_str) != Some(APP)
            || manifest.get("version").and_then(Value::as_u64) != Some(1)
            || slides.len() > 100
            || model
                .get("media")
                .and_then(Value::as_array)
                .is_none_or(|a| !a.is_empty())
            || model
                .get("embeddedFonts")
                .and_then(Value::as_array)
                .is_none_or(|a| !a.is_empty())
        {
            return Err("Unsupported DeckCraft document");
        }
        Ok(Self {
            slides: slides.len() as u32,
        })
    }
    pub fn metadata(&self, title: &str) -> String {
        json!({"title":title,"slides":self.slides,"engineRevision":ENGINE_REVISION}).to_string()
    }
}

#[cfg(test)]
mod tests;
