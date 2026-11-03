use super::{Digest, Sha256, Value};

fn string_field(digest: &mut Sha256, text: &str) {
    digest.update((text.len() as u64).to_le_bytes());
    digest.update(text.as_bytes());
}

fn integer_field(value: &Value) -> Result<u32, String> {
    let number = value.as_u64().ok_or("invalid observed plan integer")?;
    u32::try_from(number).map_err(|_| "observed plan integer exceeds u32".into())
}

/// Native's exact `PlanDoc` binding: presentation IDs and relay confirmation stay out.
pub(super) fn observed_hash(blocks: &[Value]) -> Result<String, String> {
    let mut digest = Sha256::new();
    digest.update(b"zoen-plan-observed-v1\0");
    digest.update((blocks.len() as u64).to_le_bytes());
    for block in blocks {
        let content = &block["content"];
        if let Some(title) = content.get("Title") {
            digest.update([1]);
            match title["budget_cents"].as_str() {
                Some(cents) => {
                    digest.update([1]);
                    digest.update(
                        cents
                            .parse::<i64>()
                            .map_err(|_| "invalid observed plan cents")?
                            .to_le_bytes(),
                    );
                }
                None if title["budget_cents"].is_null() => digest.update([0]),
                None => return Err("invalid observed plan budget".into()),
            }
        } else if content == "Summary" {
            digest.update([2]);
        } else if content == "Section" {
            digest.update([3]);
        } else if let Some(line) = content.get("Line") {
            digest.update([4]);
            string_field(
                &mut digest,
                line["legacy_id"]
                    .as_str()
                    .ok_or("invalid observed plan field")?,
            );
            digest.update(
                line["cost_cents"]
                    .as_str()
                    .ok_or("invalid observed plan field")?
                    .parse::<i64>()
                    .map_err(|_| "invalid observed plan cents")?
                    .to_le_bytes(),
            );
            digest.update([u8::from(
                line["done"]
                    .as_bool()
                    .ok_or("invalid observed plan field")?,
            )]);
        } else if let Some(prose) = content.get("Prose") {
            digest.update([5]);
            hash_prose(&mut digest, &prose["kind"])?;
        } else {
            return Err("invalid observed plan content".into());
        }
        string_field(
            &mut digest,
            block["text"]
                .as_str()
                .ok_or("invalid observed plan field")?,
        );
        let spans = block["spans"]
            .as_array()
            .ok_or("invalid observed plan field")?;
        digest.update((spans.len() as u64).to_le_bytes());
        for span in spans {
            digest.update(integer_field(&span["start"])?.to_le_bytes());
            digest.update(integer_field(&span["end"])?.to_le_bytes());
            string_field(
                &mut digest,
                span["key"].as_str().ok_or("invalid observed plan field")?,
            );
            string_field(
                &mut digest,
                span["value"]
                    .as_str()
                    .ok_or("invalid observed plan field")?,
            );
        }
    }
    Ok(hex::encode(digest.finalize()))
}

fn hash_prose(digest: &mut Sha256, kind: &Value) -> Result<(), String> {
    match kind.as_str() {
        Some("Paragraph") => digest.update([1]),
        Some("Quote") => digest.update([6]),
        Some("Divider") => digest.update([8]),
        Some("Raw") => digest.update([10]),
        _ => {
            if let Some(heading) = kind.get("Heading") {
                digest.update([2]);
                digest.update(integer_field(&heading["level"])?.to_le_bytes());
                digest.update([u8::from(
                    heading["setext"]
                        .as_bool()
                        .ok_or("invalid observed prose field")?,
                )]);
            } else if let Some(bullet) = kind.get("Bullet") {
                digest.update([3]);
                digest.update(integer_field(&bullet["indent"])?.to_le_bytes());
            } else if let Some(numbered) = kind.get("Numbered") {
                digest.update([4]);
                digest.update(integer_field(&numbered["indent"])?.to_le_bytes());
                digest.update(integer_field(&numbered["number"])?.to_le_bytes());
            } else if let Some(task) = kind.get("Task") {
                digest.update([5]);
                digest.update(integer_field(&task["indent"])?.to_le_bytes());
                digest.update([u8::from(
                    task["checked"]
                        .as_bool()
                        .ok_or("invalid observed prose field")?,
                )]);
            } else if let Some(code) = kind.get("Code") {
                digest.update([7]);
                string_field(
                    digest,
                    code["lang"]
                        .as_str()
                        .ok_or("invalid observed prose field")?,
                );
            } else if let Some(image) = kind.get("Image") {
                digest.update([9]);
                for key in ["url", "alt", "title"] {
                    string_field(
                        digest,
                        image[key].as_str().ok_or("invalid observed prose field")?,
                    );
                }
            } else {
                return Err("invalid observed prose kind".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::observed_hash;
    use serde_json::{json, Value};

    fn fixture() -> Result<Vec<Value>, String> {
        serde_json::from_str(r#"[{"content": {"Title": {"budget_cents": "-42"}}, "text": "Plano á", "spans": []}, {"content": "Summary", "text": "summary", "spans": []}, {"content": "Section", "text": "section", "spans": []}, {"content": {"Line": {"legacy_id": "line-1", "cost_cents": "123", "done": true}}, "text": "line", "spans": [{"start": 0, "end": 4, "key": "b", "value": ""}]}, {"content": {"Prose": {"kind": "Paragraph"}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Heading": {"level": 2, "setext": true}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Bullet": {"indent": 3}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Numbered": {"indent": 4, "number": 7}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Task": {"indent": 5, "checked": false}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": "Quote"}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Code": {"lang": "rust"}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": "Divider"}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": {"Image": {"url": "image", "alt": "alt", "title": "title"}}}}, "text": "prose", "spans": []}, {"content": {"Prose": {"kind": "Raw"}}, "text": "prose", "spans": []}]"#).map_err(|error| error.to_string())
    }

    #[test]
    fn binding_preserves_all_legacy_content_tags_and_spans() -> Result<(), String> {
        // Frozen output of the pre-refactor byte encoding, including UTF-8,
        // signed cents, all prose tags and formatting offsets.
        assert_eq!(
            observed_hash(&fixture()?)?,
            "b6802ab80c652927c82ad1c1b5dcbca315bbe22608819d6404b1a8c0ef380f8a"
        );
        Ok(())
    }

    #[test]
    fn invalid_fields_are_refused_instead_of_truncated_or_panicking() -> Result<(), String> {
        let mut blocks = fixture()?;
        blocks[3]["spans"][0]["end"] = json!(u64::from(u32::MAX) + 1);
        assert!(observed_hash(&blocks).is_err());
        let mut blocks = fixture()?;
        blocks[3]["content"]["Line"]["cost_cents"] = json!("invalid");
        assert!(observed_hash(&blocks).is_err());
        let mut blocks = fixture()?;
        blocks[4]["content"]["Prose"]["kind"] = json!("Unknown");
        assert!(observed_hash(&blocks).is_err());
        let mut blocks = fixture()?;
        blocks[0]["content"]["Title"]["budget_cents"] = json!(false);
        assert!(observed_hash(&blocks).is_err());
        Ok(())
    }
}
