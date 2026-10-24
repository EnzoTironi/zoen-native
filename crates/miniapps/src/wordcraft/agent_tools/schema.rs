//! Exact model-facing schemas; no arbitrary command/file/network surface.
use super::{json, Definition, Value};
fn range() -> Value {
    let position = json!({"type":"object","properties":{"paragraph":{"type":"integer","minimum":0,"maximum":4294967295_u32},"offset":{"type":"integer","minimum":0,"maximum":4294967295_u32}},"required":["paragraph","offset"],"additionalProperties":false});
    json!({"type":"object","description":"Zero-based body paragraph indices and UTF-8 byte offsets from read_wordcraft; end exclusive, ordered and on codepoint boundaries. No tables or non-body stories.","properties":{"start":position,"end":position},"required":["start","end"],"additionalProperties":false})
}
fn format() -> Value {
    let mut props = json!({"bold":{"type":"boolean"},"italic":{"type":"boolean"},"underline":{"type":"boolean"},"strikethrough":{"type":"boolean"},"font":{"type":"string","minLength":1,"maxLength":128},"size":{"type":"number","minimum":1,"maximum":1638,"multipleOf":0.5},"color":{"type":"string","pattern":"^(auto|[a-fA-F0-9]{6})$"},"alignment":{"type":"string","enum":["left","center","right","justify"]},"paragraphStyle":{"type":"string","minLength":1,"maxLength":128}});
    for name in ["spaceBefore", "spaceAfter"] {
        props[name] = json!({"type":"number","minimum":0,"maximum":1584});
    }
    props["lineSpacing"] = json!({"type":"number","minimum":0.5,"maximum":10});
    json!({"type":"object","properties":props,"minProperties":1,"additionalProperties":false})
}
fn write(name: &str, description: &str, extra: Value) -> Definition {
    let mut properties = json!({"artifactId":{"type":"string","minLength":1,"maxLength":128},"version":{"type":"integer","minimum":1,"maximum":4294967294_u32},"sha256":{"type":"string","pattern":"^[a-f0-9]{64}$"}});
    let mut required = vec![json!("artifactId"), json!("version"), json!("sha256")];
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            properties[key] = value.clone();
            required.push(json!(key));
        }
    }
    Definition {
        name: name.into(),
        description: description.into(),
        parameters: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
    }
}
pub(super) fn tools() -> Vec<Definition> {
    let text = json!({"type":"string","maxLength":16384});
    vec![
        Definition{name:"read_wordcraft".into(),description:"Read the exact accepted DOCX head/hash and text. Rich paragraph/run/style details share text ranges and include explicit omission/counts when they exceed the bounded reply. Unreported formatting remains preserved.".into(),parameters:json!({"type":"object","properties":{"artifactId":{"type":"string","minLength":1,"maxLength":128}},"required":["artifactId"],"additionalProperties":false})},
        write("wordcraft_text_replace","Replace all body text intentionally. Prefer wordcraft_edit_range for local edits that preserve surrounding content/styles. Requires exact head and owner review.",json!({"text":text})),
        write("wordcraft_edit_range","Replace or insert text at an exact body range, preserving the rest of the rich document. LF creates paragraphs. Requires exact head and owner review.",json!({"range":range(),"text":text})),
        write("wordcraft_format_range","Apply explicit character/paragraph formatting to a nonempty exact body range. Unspecified properties remain unchanged. One atomic Undo, exact head and owner review.",json!({"range":range(),"format":format()})),
    ]
}
