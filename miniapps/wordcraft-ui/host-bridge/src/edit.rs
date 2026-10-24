//! Translate a closed reviewed operation into genuine Session commands.
use crate::{Action, Result, failure};
use serde_json::{Value, json};
use wordcraft_doc::{Block, Pos};
use wordcraft_engine::Session;
use zoen_word_profile::action::{Format, Position, Range};

fn position(s: &Session, p: &Position) -> Result<Pos> {
    let block = usize::try_from(p.paragraph).map_err(|_| failure("invalid_range"))?;
    let off = usize::try_from(p.offset).map_err(|_| failure("invalid_range"))?;
    let Some(Block::Para(para)) = s.doc.body.get(block).map(AsRef::as_ref) else {
        return Err(failure("invalid_range"));
    };
    if off > para.text.len() || !para.text.is_char_boundary(off) {
        return Err(failure("invalid_range"));
    }
    Ok(Pos::body(block, off))
}
fn select(s: &mut Session, range: &Range) -> Result<()> {
    let a = position(s, &range.start)?;
    let b = position(s, &range.end)?;
    // Refuse a range crossing a table/other body block instead of flattening it.
    for index in range.start.paragraph..=range.end.paragraph {
        if !matches!(
            s.doc
                .body
                .get(usize::try_from(index).map_err(|_| failure("invalid_range"))?)
                .map(AsRef::as_ref),
            Some(Block::Para(_))
        ) {
            return Err(failure("unsupported_range"));
        }
    }
    run(s, "select.range", &json!({"anchor":a,"focus":b}))
}
fn run(s: &mut Session, id: &str, params: &Value) -> Result<()> {
    s.run(id, params)
        .map(|_| ())
        .map_err(|_| failure("engine_edit"))
}
fn text(s: &mut Session, text: &str) -> Result<()> {
    // LF denotes paragraphs; upstream raw typing LF denotes a soft break.
    if text.contains('\n') {
        run(s, "edit.pasteText", &json!({"text":text}))
    } else {
        run(s, "text.insert", &json!({"text":text,"raw":true}))
    }
}
fn format(s: &mut Session, f: &Format) -> Result<()> {
    let mut commands = Vec::new();
    // Apply paragraph style before direct formatting, matching the visible editor.
    if let Some(style) = &f.paragraph_style {
        commands.push(("para.style", json!({"style":style})));
    }
    for (id, value) in [
        ("format.bold", f.bold),
        ("format.italic", f.italic),
        ("format.underline", f.underline),
        ("format.strikethrough", f.strikethrough),
    ] {
        if let Some(value) = value {
            commands.push((id, json!({"value":value})));
        }
    }
    if let Some(name) = &f.font {
        commands.push(("format.font", json!({"name":name})));
    }
    if let Some(size) = f.size {
        commands.push(("format.size", json!({"size":size})));
    }
    if let Some(color) = &f.color {
        commands.push(("format.color", json!({"color":color})));
    }
    if let Some(value) = &f.alignment {
        commands.push(("para.align", json!({"value":value.command_value()})));
    }
    if f.space_before.is_some() || f.space_after.is_some() {
        let mut params = json!({});
        if let Some(n) = f.space_before {
            params["before"] = json!(n);
        }
        if let Some(n) = f.space_after {
            params["after"] = json!(n);
        }
        commands.push(("para.spacing", params));
    }
    if let Some(value) = f.line_spacing {
        commands.push(("para.lineSpacing", json!({"value":value})));
    }
    for (index, (id, params)) in commands.into_iter().enumerate() {
        if index > 0 {
            s.join_next_undo();
        }
        run(s, id, &params)?;
    }
    Ok(())
}
/// Caller snapshots the actual Session before entry and rolls back on any failure.
pub(crate) fn apply(s: &mut Session, action: &Action) -> Result<()> {
    action.validate().map_err(str::to_owned)?;
    s.close_typing();
    s.column = None;
    s.column_mode = false;
    s.math = None;
    s.pending = None;
    s.also_selected.clear();
    let result = match action {
        Action::TextReplace(v) => {
            run(s, "select.all", &json!({"whole":true}))?;
            text(s, &v.text)
        }
        Action::TextEdit(v) => {
            select(s, &v.range)?;
            text(s, &v.text)
        }
        Action::RangeFormat(v) => {
            select(s, &v.range)?;
            format(s, &v.format)
        }
    };
    if result.is_ok() {
        s.close_typing();
    }
    result
}
/// The upstream transaction preserves document/selection/Undo/Redo/typing. These
/// additional public interaction fields can be touched by our closed commands.
#[cfg(feature = "browser")]
pub(crate) fn transaction<T>(
    s: &mut Session,
    apply: impl FnOnce(&mut Session) -> Result<T>,
) -> Result<T> {
    let before = s.edit_snapshot();
    let pending = s.pending.clone();
    let column = s.column.clone();
    let math = s.math.clone();
    let also_selected = s.also_selected.clone();
    let last_command = s.last_command.clone();
    let pending_move = s.pending_move.clone();
    let status = s.status.clone();
    let goal_x = s.goal_x;
    let page_hint = s.page_hint;
    let column_mode = s.column_mode;
    let result = apply(s);
    if result.is_err() {
        s.restore(before);
        s.pending = pending;
        s.column = column;
        s.math = math;
        s.also_selected = also_selected;
        s.last_command = last_command;
        s.pending_move = pending_move;
        s.status = status;
        s.goal_x = goal_x;
        s.page_hint = page_hint;
        s.column_mode = column_mode;
    }
    result
}
