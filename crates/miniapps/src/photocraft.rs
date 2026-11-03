//! Bounded native raster container storage. The pinned engine validates/decodes
//! pixels and full document fidelity; Core preserves every accepted source byte.
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};
pub mod agent_tools;
mod manifest;
#[cfg(test)]
mod tests;

pub const APP: &str = "photocraft";
pub const RESOURCE: &str = "ui://zoen/photocraft/index.html";
pub const MIME: &str = "application/x-photocraft";
pub const ENGINE_REVISION: &str = "9b9643764f3fb77d9f0e48d7510983ae50621e11";
pub const MAX_BYTES: usize = 8 << 20;
pub const EMPTY: &[u8] = include_bytes!("../resources/photocraft-empty.pcraft");
const MAX_MANIFEST: usize = 256 << 10;
const INVALID: &str = "Invalid or unsupported managed raster document";

pub struct Document {
    pub width: u32,
    pub height: u32,
    pub layers: usize,
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn container(bytes: &[u8]) -> Result<(Value, BTreeSet<String>), &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(INVALID);
    }
    let count = crate::wordcraft::archive::preflight(bytes, 128, MAX_BYTES, MAX_BYTES)
        .map_err(|_| INVALID)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| INVALID)?;
    if zip.len() != count {
        return Err(INVALID);
    }
    let mut entries = BTreeSet::new();
    let mut manifest = None;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| INVALID)?;
        let name = entry.name().to_owned();
        let tile = name
            .strip_prefix("tiles/")
            .and_then(|v| v.strip_suffix(".zst"))
            .is_some_and(hash);
        if !entries.insert(name.clone())
            || entry.is_dir()
            || (!tile && name != "manifest.json")
            || entry.compression() != zip::CompressionMethod::Stored
        {
            return Err(INVALID);
        }
        if name == "manifest.json" {
            if entry.size() > MAX_MANIFEST as u64 {
                return Err(INVALID);
            }
            let mut raw = Vec::new();
            entry
                .by_ref()
                .take((MAX_MANIFEST + 1) as u64)
                .read_to_end(&mut raw)
                .map_err(|_| INVALID)?;
            if raw.len() > MAX_MANIFEST
                || u64::try_from(raw.len()).map_err(|_| INVALID)? != entry.size()
            {
                return Err(INVALID);
            }
            manifest = Some(self::manifest::parse(&raw)?);
        }
    }
    Ok((manifest.ok_or(INVALID)?, entries))
}
impl Document {
    /// Checks ZIP32, entry bounds, exact native template structure, referenced tile
    /// names/coordinates and scalar dimensions. Does not decompress/hash pixel tiles.
    /// # Errors
    /// Refuses malformed/unknown containers or fields outside the bounded raster profile.
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let (input, entries) = container(bytes)?;
        let (mut template, _) = container(EMPTY)?;
        let number = |path| input.pointer(path).and_then(Value::as_u64).ok_or(INVALID);
        let width = u32::try_from(number("/document/size/width")?).map_err(|_| INVALID)?;
        let height = u32::try_from(number("/document/size/height")?).map_err(|_| INVALID)?;
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Err(INVALID);
        }
        number("/document/id")?;
        number("/document/layers/0/id")?;
        for path in ["/document/name", "/document/layers/0/name"] {
            if input
                .pointer(path)
                .and_then(Value::as_str)
                .is_none_or(|s| s.len() > 512 || s.chars().any(char::is_control))
            {
                return Err(INVALID);
            }
        }
        if input
            .pointer("/document/resolution_dpi")
            .and_then(Value::as_f64)
            .is_none_or(|n| !n.is_finite() || n <= 0.0 || n > 9600.0)
        {
            return Err(INVALID);
        }
        let default = input
            .pointer("/document/layers/0/content/surface/default")
            .and_then(Value::as_str)
            .ok_or(INVALID)?;
        if default.len() != 8
            || !default
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(INVALID);
        }
        let tiles = input
            .pointer("/document/layers/0/content/surface/tiles")
            .and_then(Value::as_array)
            .ok_or(INVALID)?;
        if tiles.len() > 64 {
            return Err(INVALID);
        }
        let mut referenced = BTreeSet::from(["manifest.json".into()]);
        let mut coordinates = BTreeSet::new();
        for tile in tiles {
            let tile = tile.as_object().ok_or(INVALID)?;
            if tile.len() != 3 {
                return Err(INVALID);
            }
            let x = tile.get("tx").and_then(Value::as_i64).ok_or(INVALID)?;
            let y = tile.get("ty").and_then(Value::as_i64).ok_or(INVALID)?;
            let h = tile.get("hash").and_then(Value::as_str).ok_or(INVALID)?;
            if !(-8..=8).contains(&x)
                || !(-8..=8).contains(&y)
                || !hash(h)
                || !coordinates.insert((x, y))
            {
                return Err(INVALID);
            }
            referenced.insert(format!("tiles/{h}.zst"));
        }
        if entries != referenced {
            return Err(INVALID);
        }
        // The real upstream writer seed supplies all known defaults. Only these
        // original scalar/raster fields vary in this candidate profile. Comparing
        // whole objects refuses unknown fields; it never normalizes caller bytes.
        for path in [
            "/document/id",
            "/document/name",
            "/document/size/width",
            "/document/size/height",
            "/document/resolution_dpi",
            "/document/layers/0/id",
            "/document/layers/0/name",
            "/document/layers/0/content/surface/default",
            "/document/layers/0/content/surface/tiles",
        ] {
            *template.pointer_mut(path).ok_or(INVALID)? =
                input.pointer(path).ok_or(INVALID)?.clone();
        }
        if input != template {
            return Err(INVALID);
        }
        Ok(Self {
            width,
            height,
            layers: 1,
        })
    }
    #[must_use]
    pub fn metadata(&self, title: &str) -> String {
        json!({"title":title,"width":self.width,"height":self.height,"layers":self.layers,
            "profile":"raster-canvas-v1","engineRevision":ENGINE_REVISION})
        .to_string()
    }
}
