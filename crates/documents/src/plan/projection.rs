use super::{
    edit, error, stable_id, text_container, BlockKind, Container, ContainerTrait, Digest, HashSet,
    LineView, LoroMap, LoroValue, Node, Page, PlanLine, PlanView, Result, SectionView, Sha256,
    TotalCents, ValueOrContainer, SCHEMA,
};

pub(super) fn string(map: &LoroMap, key: &str) -> Result<String> {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Ok(s.to_string()),
        _ => Err(error(format!("missing or invalid plan field {key}"))),
    }
}

pub(super) fn integer(map: &LoroMap, key: &str) -> Result<i64> {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::I64(n))) => Ok(n),
        _ => Err(error(format!("missing or invalid integer {key}"))),
    }
}

pub(super) fn budget(map: &LoroMap) -> Result<Option<i64>> {
    match map.get("budget_cents") {
        Some(ValueOrContainer::Value(LoroValue::Null)) => Ok(None),
        Some(ValueOrContainer::Value(LoroValue::I64(n))) => Ok(Some(n)),
        _ => Err(error("missing or invalid budget")),
    }
}

pub(super) fn line(node: &Node) -> Result<PlanLine> {
    if node.kind != BlockKind::Line {
        return Err(error("not a plan line"));
    }
    let Some(ValueOrContainer::Value(LoroValue::Bool(done))) = node.map.get("done") else {
        return Err(error("missing or invalid done"));
    };
    Ok(PlanLine {
        id: string(&node.map, "line_id")?,
        text: node.text.to_string(),
        cost_cents: integer(&node.map, "cost_cents")?,
        done,
    })
}

pub(super) fn read_nodes(page: &Page, root: &str) -> Result<Vec<Node>> {
    let meta = page.doc.get_map("page");
    if string(&meta, "schema")? != SCHEMA || string(&meta, "root")? != root {
        return Err(error("plan seed identity mismatch"));
    }
    if meta
        .keys()
        .any(|key| !matches!(key.as_str(), "schema" | "root"))
    {
        return Err(error("unknown plan root attribute"));
    }
    let list = page.blocks_list();
    let mut result = Vec::with_capacity(list.len());
    let mut ids = HashSet::new();
    let mut texts = HashSet::new();
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(map))) = list.get(i) else {
            return Err(error("plan blocks must be maps"));
        };
        let node = read_node(map)?;
        if !ids.insert(node.id.clone()) || !texts.insert(node.text.id()) {
            return Err(error("invalid or duplicate internal block ID"));
        }
        result.push(node);
    }
    if result.len() < 2
        || result[0].kind != BlockKind::Title
        || result[1].kind != BlockKind::Summary
        || result[0].id != stable_id(root, "title", "", 0)?
        || result[1].id != stable_id(root, "summary", "", 0)?
        || result[2..]
            .iter()
            .any(|n| matches!(n.kind, BlockKind::Title | BlockKind::Summary))
    {
        return Err(error(
            "plan title and summary anchors must remain unique and first",
        ));
    }
    Ok(result)
}

pub(super) fn parse_role(value: &str) -> Result<BlockKind> {
    match value {
        "plan_title" => Ok(BlockKind::Title),
        "plan_summary" => Ok(BlockKind::Summary),
        "plan_section" => Ok(BlockKind::Section),
        "plan_line" => Ok(BlockKind::Line),
        "plan_prose" => Ok(BlockKind::Prose),
        _ => Err(error("unsupported typed plan block")),
    }
}

pub(super) fn role_key(kind: BlockKind, key: &str) -> bool {
    matches!(key, "id" | "kind" | "text")
        || match kind {
            BlockKind::Title => key == "budget_cents",
            BlockKind::Line => matches!(key, "line_id" | "cost_cents" | "done"),
            BlockKind::Prose => key == "prose_kind",
            BlockKind::Summary | BlockKind::Section => false,
        }
}

pub(super) fn read_node(map: LoroMap) -> Result<Node> {
    let id = string(&map, "id")?;
    if id.trim().is_empty() || id.len() > crate::MAX_BLOCK_ID_BYTES {
        return Err(error("invalid internal block ID"));
    }
    let kind = parse_role(&string(&map, "kind")?)?;
    if map.keys().any(|key| !role_key(kind, key.as_str())) {
        return Err(error("attribute does not belong to its plan block role"));
    }
    let text = text_container(&map)?;
    let node = Node {
        id,
        kind,
        map,
        text,
    };
    match kind {
        BlockKind::Title => {
            budget(&node.map)?;
        }
        BlockKind::Line => {
            line(&node)?;
        }
        BlockKind::Prose => {
            edit::parse_prose_kind(&string(&node.map, "prose_kind")?)?;
        }
        BlockKind::Summary | BlockKind::Section => {}
    }
    edit::read_runs(&node.text)?;
    Ok(node)
}

pub(super) fn line_hash(line: &PlanLine) -> Result<String> {
    let bytes = serde_json::to_vec(&(&line.text, line.cost_cents)).map_err(error)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn project(page: &Page, root: &str) -> Result<PlanView> {
    let nodes = read_nodes(page, root)?;
    let mut sections: Vec<SectionView> = Vec::new();
    let mut sum = Some(0_i128);
    for node in &nodes[2..] {
        match node.kind {
            BlockKind::Section => sections.push(SectionView {
                block_id: Some(node.id.clone()),
                title: node.text.to_string(),
                recovered: false,
                lines: Vec::new(),
            }),
            BlockKind::Line => {
                if sections.is_empty() {
                    sections.push(SectionView {
                        block_id: None,
                        title: "Recovered lines".into(),
                        recovered: true,
                        lines: Vec::new(),
                    });
                }
                let line = line(node)?;
                sum = sum.and_then(|n| n.checked_add(i128::from(line.cost_cents)));
                let content_hash = line_hash(&line)?;
                sections
                    .last_mut()
                    .ok_or_else(|| error("line section missing"))?
                    .lines
                    .push(LineView {
                        block_id: node.id.clone(),
                        line,
                        content_hash,
                    });
            }
            BlockKind::Prose => {}
            _ => return Err(error("unexpected anchor inside plan body")),
        }
    }
    let total = sum
        .and_then(|n| i64::try_from(n).ok())
        .map_or(TotalCents::Overflow, TotalCents::Cents);
    Ok(PlanView {
        title: nodes[0].text.to_string(),
        summary: nodes[1].text.to_string(),
        budget_cents: budget(&nodes[0].map)?,
        sections,
        total,
    })
}
