use crate::{Error, MAX_DOCUMENT_BYTES, MAX_MANIFEST_BYTES, format_error};
use photocraft_doc::{ColorMode, PixelFormat, SampleType};
use photocraft_format::{FORMAT_VERSION, Manifest, manifest::ContentM, zip::ZipReader};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct Checked {
    pub document: Value,
    pub entries: BTreeSet<String>,
}

pub(super) fn check(bytes: &[u8]) -> Result<Checked, Error> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Limit);
    }
    let zip = ZipReader::new(bytes).map_err(|error| format_error(&error))?;
    if zip.entries.len() > 128 {
        return Err(Error::Limit);
    }
    let mut entries = BTreeSet::new();
    for entry in &zip.entries {
        let name = &entry.name;
        let blob = name
            .strip_prefix("tiles/")
            .or_else(|| name.strip_prefix("blobs/"));
        let addressed = blob.and_then(|x| x.strip_suffix(".zst")).is_some_and(|x| {
            x.len() == 64
                && x.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if (name != "manifest.json" && !addressed) || !entries.insert(name.clone()) {
            return Err(Error::UnsupportedProfile);
        }
    }
    let raw = zip
        .read_by_name("manifest.json", MAX_MANIFEST_BYTES)
        .map_err(|error| format_error(&error))?;
    let value: Value = serde_json::from_slice(&raw).map_err(|_| Error::InvalidDocument)?;
    let model: Manifest = serde_json::from_slice(&raw).map_err(|_| Error::InvalidDocument)?;
    let doc = &model.document;
    let layer = doc.layers.first().ok_or(Error::UnsupportedProfile)?;
    let ContentM::Raster { surface } = &layer.content else {
        return Err(Error::UnsupportedProfile);
    };
    if doc.mode != ColorMode::Rgb
        || doc.depth != SampleType::U8
        || surface.format != PixelFormat::RGBA8
        || doc.layers.len() != 1
        || layer.mask.is_some()
        || layer.vector_mask.is_some()
        || layer.video.is_some()
        || layer.fill_cache.is_some()
        || !layer.effects.items.is_empty()
        || !doc.channels.is_empty()
        || doc.selection.is_some()
        || doc.quick_mask.is_some()
        || !doc.patterns.is_empty()
        || doc.timeline.is_some()
        || surface
            .tiles
            .iter()
            .any(|tile| !(-8..=8).contains(&tile.tx) || !(-8..=8).contains(&tile.ty))
    {
        return Err(Error::UnsupportedProfile);
    }
    if model.format_version != FORMAT_VERSION
        || serde_json::to_value(model).map_err(|_| Error::InvalidDocument)? != value
    {
        return Err(Error::UnsupportedProfile);
    }
    let document = value
        .get("document")
        .cloned()
        .ok_or(Error::InvalidDocument)?;
    Ok(Checked { document, entries })
}
