//! Initial executable profile. Not a replacement for host sandbox/CAS checks.
use crate::profile;
use gridcraft_engine::{EngineError, Session};
use serde_json::Value;
fn denied() -> EngineError {
    EngineError::Other("This action is unavailable in this workbook.".into())
}
pub fn managed_command_policy(s: &Session, id: &str, p: &Value) -> gridcraft_engine::Result<()> {
    let read = matches!(id, "document.inspect" | "cell.get" | "history.list");
    if s.managed_frozen && !read {
        return Err(denied());
    }
    match id {
        "cell.set" => {
            let object = p.as_object().ok_or_else(denied)?;
            if object
                .keys()
                .any(|k| !matches!(k.as_str(), "sheet" | "cell" | "input" | "array"))
                || p.get("array").is_some_and(|v| v != &Value::Bool(false))
            {
                return Err(denied());
            }
            let input = p.get("input").and_then(Value::as_str).ok_or_else(denied)?;
            if input.len() > 16 * 1024
                || input.chars().any(|c| c == '\0')
                || profile::validate_cell_input(input).is_err()
            {
                return Err(denied());
            }
            let d = s.doc()?;
            let sheet = match p.get("sheet") {
                Some(Value::String(name)) => {
                    d.wb.sheets
                        .iter()
                        .position(|sheet| sheet.name == *name)
                        .ok_or_else(denied)?
                }
                Some(value) => value
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .filter(|v| *v < d.wb.sheets.len())
                    .ok_or_else(denied)?,
                None => d.wb.active_sheet,
            };
            let cell = p
                .get("cell")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| d.selection.active.a1());
            if !profile::cell_reference(&cell) {
                return Err(denied());
            }
            let target = d.wb.sheet(sheet).ok_or_else(denied)?;
            let at = gridcraft_engine::core::CellRef::parse(&cell).ok_or_else(denied)?;
            let total =
                d.wb.sheets
                    .iter()
                    .try_fold(0usize, |n, sh| n.checked_add(sh.cells.len()))
                    .ok_or_else(denied)?;
            if target.cell(at).is_none() && total >= profile::MAX_CELLS {
                return Err(denied());
            }
        }
        "cell.get" => {
            if p.get("cell")
                .and_then(Value::as_str)
                .is_some_and(|c| !profile::cell_reference(c))
            {
                return Err(denied());
            }
        }
        "edit.undo" | "edit.redo" => {
            if p.as_object()
                .is_none_or(|obj| obj.keys().any(|k| k != "steps"))
                || p.get("steps").is_some_and(|v| v.as_u64() != Some(1))
            {
                return Err(denied());
            }
        }
        "sheet.activate" | "sheet.next" | "sheet.previous" | "selection.set" | "selection.move"
        | "selection.next" | "document.inspect" | "history.list" => {}
        _ => return Err(denied()),
    }
    Ok(())
}
