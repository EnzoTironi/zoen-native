//! Closed bounded model contracts. Model data cannot select authority or hosting.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Selected {
    pub artifact_id: String,
    pub version: u32,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Create {
    pub title: String,
    pub html: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Replace {
    pub artifact_id: String,
    pub version: u32,
    pub sha256: String,
    pub html: String,
}
#[derive(Clone, Debug)]
pub enum Command {
    Create(Create),
    Read(Selected),
    Replace(Replace),
}
impl Command {
    pub fn artifact(&self) -> Option<&str> {
        match self {
            Self::Create(_) => None,
            Self::Read(c) => Some(&c.artifact_id),
            Self::Replace(c) => Some(&c.artifact_id),
        }
    }
    pub fn is_write(&self) -> bool {
        !matches!(self, Self::Read(_))
    }
}
pub fn executor_binding() -> String {
    sha(b"zoen.native-site.file.v1\0create_site\0read_site\0replace_site_source\0no-publication")
}
pub fn recognizes(tool: &str) -> bool {
    matches!(tool, "create_site" | "read_site" | "replace_site_source")
}
pub fn model_name(tool: &str, item: &str) -> Option<String> {
    if !recognizes(tool) || !token(item) {
        return None;
    }
    Some(if tool == "create_site" {
        tool.into()
    } else {
        format!("{tool}_{}", &sha(item.as_bytes())[..16])
    })
}
pub fn is_model_name(name: &str) -> bool {
    name == "create_site"
        || ["read_site_", "replace_site_source_"].iter().any(|p| {
            name.strip_prefix(p)
                .is_some_and(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        })
}
fn malformed(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: format!("Invalid private Site arguments: {reason}."),
    }
}
pub fn parse(tool: &str, args: &Value) -> R<Command> {
    if serde_json::to_vec(args)
        .map_err(|_| malformed("encoding"))?
        .len()
        > ARGUMENT_LIMIT
    {
        return Err(malformed("capacity"));
    }
    let command = match tool {
        "create_site" => {
            let c: Create =
                serde_json::from_value(args.clone()).map_err(|_| malformed("create schema"))?;
            if c.title.trim() != c.title
                || c.title.is_empty()
                || c.title.chars().count() > 160
                || c.title.len() > 240
                || c.title.chars().any(char::is_control)
                || c.title.contains(['/', '\\'])
                || !html_valid(&c.html)
            {
                return Err(malformed("title or HTML"));
            }
            Command::Create(c)
        }
        "read_site" => Command::Read(serde_json::from_value(args.clone()).map_err(|_| invalid())?),
        "replace_site_source" => {
            let c: Replace = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if !html_valid(&c.html) {
                return Err(invalid());
            }
            Command::Replace(c)
        }
        _ => return Err(invalid()),
    };
    let selected = match &command {
        Command::Create(_) => None,
        Command::Read(c) => Some((&c.artifact_id, c.version, &c.sha256)),
        Command::Replace(c) => Some((&c.artifact_id, c.version, &c.sha256)),
    };
    if selected.is_some_and(|(item, v, hash)| !token(item) || v == 0 || !digest(hash)) {
        return Err(invalid());
    }
    Ok(command)
}
pub fn definitions() -> Vec<Value> {
    let mut selected = json!({"artifactId":{"type":"string"},"version":{"type":"integer","minimum":1},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}});
    let read = json!({"name":"read_site","description":"Read the exact saved private Site version. Does not publish or read an unsaved draft.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId","version","sha256"],"properties":selected}});
    selected["html"] = json!({"type":"string","minLength":1,"maxLength":16384});
    vec![
        json!({"name":"create_site","description":"Propose one private Site in this conversation. Provide complete HTML, not a hosting URL. Requires the owner's exact review and never publishes. Call alone, then read the created version.","parameters":{"type":"object","additionalProperties":false,"required":["title","html"],"properties":{"title":{"type":"string","minLength":1,"maxLength":160},"html":{"type":"string","minLength":1,"maxLength":16384}}}}),
        read,
        json!({"name":"replace_site_source","description":"Propose replacement HTML for the exact saved Site version/hash just read. Requires owner review. Preserves private history; does not publish.","parameters":{"type":"object","additionalProperties":false,"required":["artifactId","version","sha256","html"],"properties":selected}}),
    ]
}
pub fn definition(tool: &str, item: &str) -> R<Value> {
    let mut value = definitions()
        .into_iter()
        .find(|v| v["name"] == tool)
        .ok_or_else(invalid)?;
    value["name"] = json!(model_name(tool, item).ok_or_else(invalid)?);
    if tool != "create_site" {
        value["parameters"]["properties"]["artifactId"]["enum"] = json!([item]);
    }
    Ok(value)
}

pub fn selected_definition(tool: &str, item: &str, version: u32, hash: &str) -> R<Value> {
    if version == 0 || !digest(hash) || tool == "create_site" {
        return Err(invalid());
    }
    let mut value = definition(tool, item)?;
    value["parameters"]["properties"]["version"]["enum"] = json!([version]);
    value["parameters"]["properties"]["sha256"]["enum"] = json!([hash]);
    Ok(value)
}
