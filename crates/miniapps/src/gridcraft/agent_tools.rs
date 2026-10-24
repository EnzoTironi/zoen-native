//! Two closed data-only operations; native turns retain all actor/source authority.
use crate::deckcraft::agent_tools::{Definition, ParsedCommand};
use serde_json::{json, Value};

pub fn tools() -> Vec<Definition> {
    let artifact = json!({"type":"string","minLength":1,"maxLength":128});
    vec![
        Definition {
            name: "read_gridcraft".into(),
            description: "Read the accepted spreadsheet head/hash, sheet names and one exact cell (or the active cell when omitted). Supply sheet and cell together. Isolated executor capacity is 512 KiB.".into(),
            parameters: json!({"type":"object","properties":{"artifactId":artifact,"sheet":{"type":"string","minLength":1,"maxLength":200},"cell":{"type":"string","minLength":2,"maxLength":8}},"required":["artifactId"],"additionalProperties":false}),
        },
        Definition {
            name: "gridcraft_cell_set".into(),
            description: "Set one cell in an existing sheet using its exact observed version/hash. Supports one reversible cell.set command, not paths, URLs or arbitrary code.".into(),
            parameters: json!({"type":"object","properties":{"artifactId":artifact,"version":{"type":"integer","minimum":1,"maximum":4294967294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"},"sheet":{"type":"string","minLength":1,"maxLength":200},"cell":{"type":"string","minLength":2,"maxLength":8},"input":{"type":"string","maxLength":16384}},"required":["artifactId","version","sha256","sheet","cell","input"],"additionalProperties":false}),
        },
    ]
}
pub fn recognizes(tool: &str) -> bool {
    matches!(tool, "read_gridcraft" | "gridcraft_cell_set")
}
pub fn command(tool: &str, value: &Value) -> Result<ParsedCommand, String> {
    if !recognizes(tool) {
        return Err("Unsupported spreadsheet operation".into());
    }
    let args = value
        .as_object()
        .ok_or("Spreadsheet arguments must be an object")?;
    let fields: &[&str] = if tool == "read_gridcraft" {
        &["artifactId", "sheet", "cell"]
    } else {
        &["artifactId", "version", "sha256", "sheet", "cell", "input"]
    };
    if (tool != "read_gridcraft" && args.len() != fields.len())
        || args.keys().any(|key| !fields.contains(&key.as_str()))
        || !args.contains_key("artifactId")
    {
        return Err("Unexpected or missing spreadsheet argument".into());
    }
    let item = args["artifactId"]
        .as_str()
        .ok_or("An exact artifactId is required")?;
    if item.is_empty()
        || item.len() > 128
        || !item
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err("An exact artifactId is required".into());
    }
    if tool == "read_gridcraft" {
        if args.len() == 1 {
            return Ok((None, None, None));
        }
        if args.len() != 3
            || args
                .get("sheet")
                .and_then(Value::as_str)
                .is_none_or(|s| s.is_empty() || s.len() > 200 || s.chars().any(char::is_control))
            || args
                .get("cell")
                .and_then(Value::as_str)
                .is_none_or(|s| !crate::gridcraft::cell_reference(s))
        {
            return Err("Supply one exact sheet and cell together".into());
        }
        return Ok((None, None, None));
    }
    let version = args["version"]
        .as_u64()
        .filter(|n| *n > 0 && *n < u64::from(u32::MAX))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("An exact observed version is required")?;
    let hash = args["sha256"]
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("An exact observed hash is required")?;
    let sheet = args["sheet"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 200 && !s.chars().any(char::is_control))
        .ok_or("An observed sheet name is required")?;
    let cell = args["cell"]
        .as_str()
        .filter(|s| crate::gridcraft::cell_reference(s))
        .ok_or("An in-profile cell is required")?;
    let input = args["input"]
        .as_str()
        .filter(|s| s.len() <= 16 * 1024 && !s.contains('\0'))
        .ok_or("Cell input exceeds the supported limit")?;
    crate::gridcraft::validate_cell_input(input).map_err(str::to_owned)?;
    let mut action =
        json!({"command":"cell.set","params":{"sheet":sheet,"cell":cell,"input":input}});
    action.sort_all_objects();
    Ok((Some(version), Some(hash.to_owned()), Some(action)))
}
