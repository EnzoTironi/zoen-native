use super::{
    block_map, block_position, error, integer, set_budget, set_text, string, text_container,
    BlockKind, Digest, HashMap, HashSet, LoroMap, LoroText, LoroValue, Page, PlanDoc, PlanLine,
    Result, Sha256, ValueOrContainer,
};

pub(super) fn stable_id(root: &str, kind: &str, legacy: &str, occurrence: usize) -> Result<String> {
    let bytes = serde_json::to_vec(&(root, kind, legacy, occurrence)).map_err(error)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

struct Desired {
    id: String,
    kind: BlockKind,
    text: String,
    line: Option<PlanLine>,
    budget: Option<i64>,
}

pub(super) fn replay_version(page: &Page, root: &str, plan: &PlanDoc) -> Result<()> {
    let mut desired = vec![
        Desired {
            id: stable_id(root, "title", "", 0)?,
            kind: BlockKind::Title,
            text: plan.title.clone(),
            line: None,
            budget: plan.budget_cents,
        },
        Desired {
            id: stable_id(root, "summary", "", 0)?,
            kind: BlockKind::Summary,
            text: plan.summary.clone(),
            line: None,
            budget: None,
        },
    ];
    let mut occurrences: HashMap<&str, usize> = HashMap::new();
    for (i, section) in plan.sections.iter().enumerate() {
        desired.push(Desired {
            id: stable_id(root, "section", "", i)?,
            kind: BlockKind::Section,
            text: section.title.clone(),
            line: None,
            budget: None,
        });
        for line in &section.lines {
            let occurrence = occurrences.entry(line.id.as_str()).or_default();
            let id = stable_id(root, "line", &line.id, *occurrence)?;
            *occurrence += 1;
            desired.push(Desired {
                id,
                kind: BlockKind::Line,
                text: line.text.clone(),
                line: Some(line.clone()),
                budget: None,
            });
        }
    }
    let list = page.blocks_list();
    let keep: HashSet<_> = desired.iter().map(|b| b.id.as_str()).collect();
    for i in (0..list.len()).rev() {
        let map = block_map(page, i)?;
        if !keep.contains(string(&map, "id")?.as_str()) {
            list.delete(i, 1).map_err(error)?;
        }
    }
    for (want, target) in desired.iter().enumerate() {
        let current = block_position(page, &target.id)?;
        let map = if let Some(now) = current {
            if now != want {
                list.mov(now, want).map_err(error)?;
            }
            block_map(page, want)?
        } else {
            let map = list.insert_container(want, LoroMap::new()).map_err(error)?;
            initialize_block(
                &map,
                &target.id,
                target.kind,
                &target.text,
                target.line.as_ref(),
                target.budget,
            )?;
            continue;
        };
        let text = text_container(&map)?;
        set_text(&text, &target.text)?;
        match target.kind {
            BlockKind::Title => set_budget(&map, target.budget)?,
            BlockKind::Line => {
                let value = target
                    .line
                    .as_ref()
                    .ok_or_else(|| error("line target missing"))?;
                if integer(&map, "cost_cents")? != value.cost_cents {
                    map.insert("cost_cents", value.cost_cents).map_err(error)?;
                }
                if !matches!(map.get("done"), Some(ValueOrContainer::Value(LoroValue::Bool(done))) if done == value.done)
                {
                    map.insert("done", value.done).map_err(error)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn initialize_block(
    map: &LoroMap,
    id: &str,
    kind: BlockKind,
    text: &str,
    line: Option<&PlanLine>,
    budget: Option<i64>,
) -> Result<()> {
    map.insert("id", id).map_err(error)?;
    map.insert("kind", kind.id()).map_err(error)?;
    let rich = map
        .insert_container("text", LoroText::new())
        .map_err(error)?;
    if !text.is_empty() {
        rich.insert(0, text).map_err(error)?;
    }
    if kind == BlockKind::Title {
        map.insert(
            "budget_cents",
            budget.map_or(LoroValue::Null, LoroValue::I64),
        )
        .map_err(error)?;
    }
    if let Some(line) = line {
        map.insert("line_id", line.id.as_str()).map_err(error)?;
        map.insert("cost_cents", line.cost_cents).map_err(error)?;
        map.insert("done", line.done).map_err(error)?;
    }
    Ok(())
}
