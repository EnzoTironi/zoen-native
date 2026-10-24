//! Closed ZIP32 central-directory preflight, before zip3 deduplicates file names.
type Result<T> = std::result::Result<T, &'static str>;
fn failure(code: &'static str) -> &'static str {
    code
}
use std::collections::BTreeSet;

fn short(bytes: &[u8], offset: usize) -> Result<u16> {
    let field = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| failure("invalid_zip"))?;
    Ok(u16::from_le_bytes([field[0], field[1]]))
}
fn long(bytes: &[u8], offset: usize) -> Result<u32> {
    let field = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| failure("invalid_zip"))?;
    Ok(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

pub fn preflight(
    bytes: &[u8],
    max_parts: usize,
    max_part: usize,
    max_total: usize,
) -> Result<usize> {
    // Writer profile has no comment, archive prefix, ZIP64 or multi-disk package.
    let end = bytes
        .len()
        .checked_sub(22)
        .ok_or_else(|| failure("invalid_zip"))?;
    if bytes.get(end..end + 4) != Some(b"PK\x05\x06")
        || short(bytes, end + 4)? != 0
        || short(bytes, end + 6)? != 0
        || short(bytes, end + 20)? != 0
    {
        return Err(failure("invalid_zip"));
    }
    let count = usize::from(short(bytes, end + 10)?);
    if count == 0 || count > max_parts || usize::from(short(bytes, end + 8)?) != count {
        return Err(failure("part_count"));
    }
    let size = usize::try_from(long(bytes, end + 12)?).map_err(|_| failure("invalid_zip"))?;
    let mut cursor = usize::try_from(long(bytes, end + 16)?).map_err(|_| failure("invalid_zip"))?;
    if cursor.checked_add(size) != Some(end) {
        return Err(failure("invalid_zip"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for _ in 0..count {
        if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") || cursor + 46 > end {
            return Err(failure("invalid_zip"));
        }
        let name_length = usize::from(short(bytes, cursor + 28)?);
        let extra = usize::from(short(bytes, cursor + 30)?);
        let comment = usize::from(short(bytes, cursor + 32)?);
        let next = cursor + 46 + name_length + extra + comment;
        if next > end
            || name_length == 0
            || short(bytes, cursor + 34)? != 0
            || short(bytes, cursor + 8)? & 1 != 0
            || !matches!(short(bytes, cursor + 10)?, 0 | 8)
        {
            return Err(failure("invalid_zip"));
        }
        let name = &bytes[cursor + 46..cursor + 46 + name_length];
        if !names.insert(name) {
            return Err(failure("invalid_part"));
        }
        let inflated = u64::from(long(bytes, cursor + 24)?);
        total = total
            .checked_add(inflated)
            .ok_or_else(|| failure("inflated_size"))?;
        if inflated > max_part as u64 || total > max_total as u64 {
            return Err(failure("inflated_size"));
        }
        cursor = next;
    }
    if cursor != end {
        return Err(failure("invalid_zip"));
    }
    Ok(count)
}
