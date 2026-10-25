use super::{
    budget, edit, error, initialize_block, line, line_hash, string, BlockKind, Container,
    ExpectedLine, LoroMap, LoroText, LoroValue, Node, Page, PlanCommand, PlanError, PlanLine,
    Result, ValueOrContainer,
};

pub(super) fn set_budget(map: &LoroMap, value: Option<i64>) -> Result<()> {
    if budget(map)? != value {
        map.insert(
            "budget_cents",
            value.map_or(LoroValue::Null, LoroValue::I64),
        )
        .map_err(error)?;
    }
    Ok(())
}

pub(super) fn block_map(page: &Page, index: usize) -> Result<LoroMap> {
    match page.blocks_list().get(index) {
        Some(ValueOrContainer::Container(Container::Map(m))) => Ok(m),
        _ => Err(error("invalid block map")),
    }
}

pub(super) fn block_position(page: &Page, id: &str) -> Result<Option<usize>> {
    for i in 0..page.blocks_list().len() {
        if string(&block_map(page, i)?, "id")? == id {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

pub(super) fn text_container(map: &LoroMap) -> Result<LoroText> {
    match map.get("text") {
        Some(ValueOrContainer::Container(Container::Text(t))) => Ok(t),
        _ => Err(error("invalid rich text container")),
    }
}

// Deterministic scalar-index edit, preserving source bytes including CRLF/U+2028.
pub(super) fn set_text(text: &LoroText, target: &str) -> Result<()> {
    let before: Vec<_> = text.to_string().chars().collect();
    let after: Vec<_> = target.chars().collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let remove = before.len() - prefix - suffix;
    if remove > 0 {
        text.delete(prefix, remove).map_err(error)?;
    }
    let add: String = after[prefix..after.len() - suffix].iter().collect();
    if !add.is_empty() {
        text.insert(prefix, &add).map_err(error)?;
    }
    Ok(())
}

pub(super) fn find<'a>(nodes: &'a [Node], block: &str) -> Result<&'a Node> {
    nodes
        .iter()
        .find(|n| n.id == block)
        .ok_or_else(|| error("plan block not found"))
}

pub(super) fn check_line(node: &Node, expected: &ExpectedLine) -> Result<PlanLine> {
    let value = line(node)?;
    if let ExpectedLine::Hash(hash) = expected {
        if line_hash(&value)? != *hash {
            return Err(PlanError::StaleLine);
        }
    }
    Ok(value)
}

pub(super) fn new_block_id() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes).map_err(error)?;
    Ok(hex::encode(bytes))
}

pub(super) fn apply_command(
    page: &Page,
    nodes: &[Node],
    command: PlanCommand,
) -> Result<Option<String>> {
    let list = page.blocks_list();
    match command {
        PlanCommand::SetTitle(value) => set_text(&nodes[0].text, &value)?,
        PlanCommand::SetSummary(value) => set_text(&nodes[1].text, &value)?,
        PlanCommand::SetBudget(value) => {
            if value.is_some_and(|n| n < 0) {
                return Err(error("new budget cannot be negative"));
            }
            set_budget(&nodes[0].map, value)?;
        }
        PlanCommand::ChangeLine {
            block,
            expected,
            patch,
        } => {
            let node = find(nodes, &block)?;
            let previous = check_line(node, &expected)?;
            if let Some(cost) = patch.cost_cents {
                if cost < 0 {
                    return Err(error("new cost cannot be negative"));
                }
                if cost != previous.cost_cents {
                    node.map.insert("cost_cents", cost).map_err(error)?;
                }
            }
            if let Some(done) = patch.done {
                if done != previous.done {
                    node.map.insert("done", done).map_err(error)?;
                }
            }
            if let Some(text) = patch.text {
                set_text(&node.text, &text)?;
            }
        }
        PlanCommand::RemoveLine { block, expected } => {
            check_line(find(nodes, &block)?, &expected)?;
            let at = block_position(page, &block)?.ok_or_else(|| error("line not found"))?;
            list.delete(at, 1).map_err(error)?;
        }
        PlanCommand::InsertLine {
            section,
            text,
            cost_cents,
        } => {
            if cost_cents < 0 {
                return Err(error("new cost cannot be negative"));
            }
            if find(nodes, &section)?.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            let start = nodes
                .iter()
                .position(|n| n.id == section)
                .ok_or_else(|| error("section missing"))?;
            let at = nodes[start + 1..]
                .iter()
                .position(|n| n.kind == BlockKind::Section)
                .map_or(nodes.len(), |i| start + 1 + i);
            let id = edit::unused_id(page)?;
            let line = PlanLine {
                id: format!("ln_{id}"),
                text: text.clone(),
                cost_cents,
                done: false,
            };
            let map = list.insert_container(at, LoroMap::new()).map_err(error)?;
            initialize_block(&map, &id, BlockKind::Line, &text, Some(&line), None)?;
            return Ok(Some(id));
        }
        PlanCommand::AddSection { title } => {
            let id = edit::unused_id(page)?;
            let map = list
                .insert_container(list.len(), LoroMap::new())
                .map_err(error)?;
            initialize_block(&map, &id, BlockKind::Section, &title, None, None)?;
            return Ok(Some(id));
        }
        PlanCommand::RenameSection { block, title } => {
            let node = find(nodes, &block)?;
            if node.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            set_text(&node.text, &title)?;
        }
        PlanCommand::RemoveSection { block } => {
            if find(nodes, &block)?.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            let start = nodes
                .iter()
                .position(|n| n.id == block)
                .ok_or_else(|| error("section missing"))?;
            let end = nodes[start + 1..]
                .iter()
                .position(|n| n.kind == BlockKind::Section)
                .map_or(nodes.len(), |i| start + 1 + i);
            list.delete(start, end - start).map_err(error)?;
        }
        PlanCommand::MoveAfter { block, after } => {
            let from = nodes
                .iter()
                .position(|n| n.id == block)
                .ok_or_else(|| error("block not found"))?;
            let anchor = nodes
                .iter()
                .position(|n| n.id == after)
                .ok_or_else(|| error("anchor not found"))?;
            if from < 2 || anchor < 1 {
                return Err(error("title/summary anchors cannot move"));
            }
            if from != anchor {
                list.mov(from, if from < anchor { anchor } else { anchor + 1 })
                    .map_err(error)?;
            }
        }
    }
    Ok(None)
}
