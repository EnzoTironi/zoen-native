//! Bounded exact package parts; no extraction paths or guessed normalization.
use crate::{MAX_DOCUMENT, MAX_INFLATED, MAX_PART, MAX_PARTS};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
pub(crate) fn parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT {
        return Err(crate::failure("document_size"));
    }
    let expected = crate::archive::preflight(bytes, MAX_PARTS, MAX_PART, MAX_INFLATED)?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| crate::failure("invalid_zip"))?;
    if archive.len() != expected {
        return Err(crate::failure("invalid_zip"));
    }
    let mut declared = 0u64;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|_| crate::failure("invalid_zip"))?;
        declared = declared
            .checked_add(entry.size())
            .ok_or_else(|| crate::failure("inflated_size"))?;
        if entry.size() > MAX_PART as u64 || declared > MAX_INFLATED as u64 {
            return Err(crate::failure("inflated_size"));
        }
    }
    let mut result = BTreeMap::new();
    let mut actual = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|_| crate::failure("invalid_zip"))?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || name.is_empty()
            || name.contains(['\\', '\0'])
            || name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || result.contains_key(&name)
        {
            return Err(crate::failure("invalid_part"));
        }
        let declared = entry.size();
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((MAX_PART + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|_| crate::failure("invalid_zip"))?;
        actual = actual
            .checked_add(data.len())
            .ok_or_else(|| crate::failure("inflated_size"))?;
        if data.len() as u64 != declared || data.len() > MAX_PART || actual > MAX_INFLATED {
            return Err(crate::failure("inflated_size"));
        }
        result.insert(name, data);
    }
    Ok(result)
}
