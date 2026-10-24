//! Streaming bounds and package references for the initial managed XLSX profile.
use super::*;
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Profile {
    pub sheets: usize,
    pub cells: usize,
    sheet_ids: BTreeSet<String>,
    sheet_targets: BTreeMap<String, String>,
    workbook_type: bool,
    office_document: bool,
}
impl Profile {
    pub fn read(&mut self, part: &str, bytes: &[u8]) -> Result<(), &'static str> {
        let text = std::str::from_utf8(bytes).map_err(|_| INVALID)?;
        let mut reader = Reader::from_str(text);
        let mut stack: Vec<Vec<u8>> = Vec::new();
        let mut roots = 0;
        let mut version = quick_xml::XmlVersion::Implicit1_0;
        let mut events = 0;
        let mut cell_keys = BTreeSet::new();
        let mut formula: Option<String> = None;
        loop {
            events += 1;
            if events > 400_000 {
                return Err(INVALID);
            }
            match reader.read_event().map_err(|_| INVALID)? {
                event @ (Event::Start(_) | Event::Empty(_)) => {
                    let empty = matches!(&event, Event::Empty(_));
                    let tag = match event {
                        Event::Start(tag) | Event::Empty(tag) => tag,
                        _ => return Err(INVALID),
                    };
                    if stack.len() >= 64 {
                        return Err(INVALID);
                    }
                    let name = tag.local_name().as_ref().to_vec();
                    if stack.is_empty() {
                        roots += 1;
                        if roots != 1 || name != root(part) {
                            return Err(INVALID);
                        }
                    }
                    let attrs = attributes(&tag, &reader, version)?;
                    if part == "[Content_Types].xml"
                        && name == b"Override"
                        && attrs.get("PartName").map(String::as_str) == Some("/xl/workbook.xml")
                    {
                        if self.workbook_type
                            || attrs.get("ContentType").map(String::as_str)
                                != Some(
                                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                                )
                        {
                            return Err(INVALID);
                        }
                        self.workbook_type = true;
                    }
                    if name == b"Relationship" {
                        let target = attrs.get("Target").ok_or(INVALID)?;
                        if attrs
                            .get("TargetMode")
                            .is_some_and(|mode| mode != "Internal")
                            || target.contains(':')
                            || target.contains('\\')
                            || target.contains('%')
                            || target.starts_with('/')
                            || target.split('/').any(|p| p == ".." || p == ".")
                        {
                            return Err(INVALID);
                        }
                        let ty = attrs.get("Type").ok_or(INVALID)?;
                        if part == "_rels/.rels" && ty.ends_with("/officeDocument") {
                            if self.office_document || target != "xl/workbook.xml" {
                                return Err(INVALID);
                            }
                            self.office_document = true;
                        }
                        if part == "xl/_rels/workbook.xml.rels" {
                            let full = format!("xl/{target}");
                            if !part_name(&full) {
                                return Err(INVALID);
                            }
                            if ty.ends_with("/worksheet") {
                                if !worksheet(&full)
                                    || self
                                        .sheet_targets
                                        .insert(attrs.get("Id").ok_or(INVALID)?.clone(), full)
                                        .is_some()
                                {
                                    return Err(INVALID);
                                }
                            } else if !ty.ends_with("/styles")
                                && !ty.ends_with("/sharedStrings")
                                && !ty.ends_with("/theme")
                            {
                                return Err(INVALID);
                            }
                        }
                    }
                    if part == "xl/workbook.xml" && name == b"sheet" {
                        self.sheets += 1;
                        if self.sheets > MAX_SHEETS
                            || stack.iter().map(Vec::as_slice).collect::<Vec<_>>()
                                != [b"workbook".as_slice(), b"sheets".as_slice()]
                            || !self
                                .sheet_ids
                                .insert(attrs.get("r:id").ok_or(INVALID)?.clone())
                        {
                            return Err(INVALID);
                        }
                    }
                    if worksheet(part) && name == b"c" {
                        let key = attrs.get("r").ok_or(INVALID)?;
                        self.cells += 1;
                        if self.cells > MAX_CELLS
                            || !cell_reference(key)
                            || !cell_keys.insert(key.clone())
                        {
                            return Err(INVALID);
                        }
                    }
                    if worksheet(part) && name == b"f" && !empty {
                        formula = Some(String::new());
                    }
                    if !empty {
                        stack.push(name);
                    }
                }
                Event::End(tag) => {
                    if stack.pop().is_none() {
                        return Err(INVALID);
                    }
                    if let Some(formula) = formula.take_if(|_| tag.local_name().as_ref() == b"f") {
                        validate_formula(&formula)?;
                    }
                }
                Event::Text(value) => {
                    if value.len() > 128 * 1024 {
                        return Err(INVALID);
                    }
                    if let Some(formula) = &mut formula {
                        formula.push_str(&value.decode().map_err(|_| INVALID)?);
                        if formula.len() > 128 * 1024 {
                            return Err(INVALID);
                        }
                    }
                }
                Event::GeneralRef(value) => {
                    let character =
                        if let Some(c) = value.resolve_char_ref().map_err(|_| INVALID)? {
                            c
                        } else {
                            let name: &[u8] = value.as_ref();
                            match name {
                                b"amp" => '&',
                                b"lt" => '<',
                                b"gt" => '>',
                                b"quot" => '"',
                                b"apos" => '\'',
                                _ => return Err(INVALID),
                            }
                        };
                    if let Some(formula) = &mut formula {
                        formula.push(character);
                        if formula.len() > 128 * 1024 {
                            return Err(INVALID);
                        }
                    }
                }
                Event::Decl(declaration) => {
                    if roots != 0 || version != quick_xml::XmlVersion::Implicit1_0 {
                        return Err(INVALID);
                    }
                    version = declaration.xml_version().map_err(|_| INVALID)?;
                    if version != quick_xml::XmlVersion::Explicit1_0 {
                        return Err(INVALID);
                    }
                }
                Event::Comment(_) => {}
                Event::Eof => break,
                // No DTDs, entity declarations, CDATA or processing instructions.
                _ => return Err(INVALID),
            }
        }
        if roots != 1 || !stack.is_empty() {
            return Err(INVALID);
        }
        Ok(())
    }
    pub fn finish(&self, parts: &BTreeMap<String, Vec<u8>>) -> Result<(), &'static str> {
        if !self.workbook_type
            || !self.office_document
            || self.sheets == 0
            || self.sheet_ids.len() != self.sheet_targets.len()
            || self.sheets != parts.keys().filter(|p| worksheet(p)).count()
            || self.sheet_ids.iter().any(|id| {
                self.sheet_targets
                    .get(id)
                    .is_none_or(|target| !parts.contains_key(target))
            })
        {
            return Err(INVALID);
        }
        let targets: BTreeSet<_> = self.sheet_targets.values().collect();
        if targets.len() != self.sheet_targets.len() {
            return Err(INVALID);
        }
        Ok(())
    }
}
fn root(part: &str) -> &[u8] {
    match part {
        "[Content_Types].xml" => b"Types",
        "_rels/.rels" | "xl/_rels/workbook.xml.rels" => b"Relationships",
        "xl/workbook.xml" => b"workbook",
        "xl/styles.xml" => b"styleSheet",
        "xl/sharedStrings.xml" => b"sst",
        "xl/theme/theme1.xml" => b"theme",
        "docProps/core.xml" => b"coreProperties",
        "docProps/app.xml" => b"Properties",
        _ => b"worksheet",
    }
}
pub(super) fn validate_formula(formula: &str) -> Result<(), &'static str> {
    let canonical: String = formula
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if ['[', ']', '|'].iter().any(|c| canonical.contains(*c))
        || ["WEBSERVICE(", "HYPERLINK(", "RTD("]
            .iter()
            .any(|s| canonical.contains(s))
    {
        return Err(INVALID);
    }
    Ok(())
}
fn attributes(
    tag: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
    version: quick_xml::XmlVersion,
) -> Result<BTreeMap<String, String>, &'static str> {
    let mut out = BTreeMap::new();
    for attr in tag.attributes() {
        let attr = attr.map_err(|_| INVALID)?;
        if out.len() >= 128 || attr.value.len() > 2048 {
            return Err(INVALID);
        }
        let key = std::str::from_utf8(attr.key.as_ref())
            .map_err(|_| INVALID)?
            .to_owned();
        let value = attr
            .decoded_and_normalized_value(version, reader.decoder())
            .map_err(|_| INVALID)?
            .into_owned();
        if out.insert(key, value).is_some() {
            return Err(INVALID);
        }
    }
    Ok(out)
}
