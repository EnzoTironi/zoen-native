//! Bounded opaque XLSX storage profile, not a spreadsheet engine or normalizer.
//! The pinned GridCraft Session interprets cells, calculates and exports. Preserve
//! every accepted byte; unsupported packages fail rather than lose content.
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

#[cfg(test)]
mod tests;
mod xml;

pub const APP: &str = "gridcraft";
pub const RESOURCE: &str = "ui://zoen/gridcraft/index.html";
pub const MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
pub const ENGINE_REVISION: &str = "b8771d51fb4da60bd48c13cf313d6d6014dc5e66";
pub const MAX_BYTES: usize = 8 << 20;
pub const MAX_SHEETS: usize = 16;
pub const MAX_CELLS: usize = 40_000;
pub const MAX_ROWS: u32 = 10_000;
pub const MAX_COLUMNS: u32 = 256;
const MAX_INFLATED: usize = 16 << 20;
const MAX_PART: usize = 4 << 20;
const INVALID: &str = "Invalid or unsupported managed XLSX document";

pub struct Document {
    pub sheets: u32,
    pub cells: u32,
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(INVALID);
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| INVALID)?;
        if zip.len() > 128 {
            return Err(INVALID);
        }
        let mut parts = BTreeMap::new();
        let mut total = 0_usize;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|_| INVALID)?;
            let name = entry.name().to_owned();
            if !part_name(&name) || parts.contains_key(&name) || entry.size() > MAX_PART as u64 {
                return Err(INVALID);
            }
            let declared = usize::try_from(entry.size()).map_err(|_| INVALID)?;
            if total.checked_add(declared).is_none_or(|n| n > MAX_INFLATED) {
                return Err(INVALID);
            }
            let mut content = Vec::new();
            entry
                .by_ref()
                .take((MAX_PART.min(MAX_INFLATED - total) + 1) as u64)
                .read_to_end(&mut content)
                .map_err(|_| INVALID)?;
            if content.len() != declared || content.len() > MAX_PART {
                return Err(INVALID);
            }
            total = total.checked_add(content.len()).ok_or(INVALID)?;
            if total > MAX_INFLATED {
                return Err(INVALID);
            }
            parts.insert(name, content);
        }
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
        ] {
            if !parts.contains_key(required) {
                return Err(INVALID);
            }
        }
        let mut profile = xml::Profile::default();
        for (name, bytes) in &parts {
            profile.read(name, bytes)?;
        }
        profile.finish(&parts)?;
        Ok(Self {
            sheets: profile.sheets as u32,
            cells: profile.cells as u32,
        })
    }
    pub fn metadata(&self, title: &str) -> String {
        json!({"title":title,"sheets":self.sheets,"cells":self.cells,"engineRevision":ENGINE_REVISION}).to_string()
    }
}
fn worksheet(name: &str) -> bool {
    name.strip_prefix("xl/worksheets/sheet")
        .and_then(|s| s.strip_suffix(".xml"))
        .is_some_and(|n| {
            !n.is_empty()
                && n.len() <= 3
                && !n.starts_with('0')
                && n.bytes().all(|b| b.is_ascii_digit())
        })
}
fn part_name(name: &str) -> bool {
    matches!(
        name,
        "[Content_Types].xml"
            | "_rels/.rels"
            | "xl/workbook.xml"
            | "xl/_rels/workbook.xml.rels"
            | "xl/styles.xml"
            | "xl/sharedStrings.xml"
            | "xl/theme/theme1.xml"
            | "docProps/core.xml"
            | "docProps/app.xml"
    ) || worksheet(name)
}
pub fn cell_reference(value: &str) -> bool {
    let column_len = value.bytes().take_while(u8::is_ascii_uppercase).count();
    if column_len == 0 || column_len > 3 || value.len() > 8 {
        return false;
    }
    let (letters, digits) = value.split_at(column_len);
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let column = letters
        .bytes()
        .fold(0_u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    digits.parse::<u32>().is_ok_and(|row| row <= MAX_ROWS) && column <= MAX_COLUMNS
}

/// Reject external/network formula inputs before invoking the managed engine.
pub fn validate_cell_input(input: &str) -> Result<(), &'static str> {
    if let Some(formula) = input.strip_prefix('=') {
        xml::validate_formula(formula)
    } else {
        Ok(())
    }
}
