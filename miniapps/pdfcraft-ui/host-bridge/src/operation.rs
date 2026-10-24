//! A closed agent capability over the real upstream edit API; no UI/file dispatcher.
use crate::{Result, failure};
use pdfcraft_engine::{AddedText, BlockStyle, DocId, Edit, NewAnnotation, Session, Shape, Style};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    AddText {
        page: usize,
        rect: [f64; 4],
        text: String,
        size: f64,
    },
    InsertPage {
        at: usize,
    },
    RotatePages {
        pages: Vec<usize>,
        degrees: i64,
    },
    ReplaceText {
        page: usize,
        block: usize,
        expected: String,
        text: String,
    },
    Annotate {
        page: usize,
        rect: [f64; 4],
        text: String,
    },
}

fn text(value: &str) -> bool {
    value.len() <= 32_768
        && !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}

/// # Errors
/// Refuses stale paragraphs, invalid pages/geometry, representation loss or engine failure.
pub fn apply(session: &mut Session, id: DocId, operation: Operation, author: &str) -> Result<()> {
    let document = session.get(id).ok_or_else(|| failure("not_open"))?;
    let count = document.info.pages.len();
    let edit = match operation {
        Operation::InsertPage { at } => {
            if at > count || count >= crate::structure::MAX_PAGES {
                return Err(failure("invalid_pages"));
            }
            Edit::InsertBlankPage {
                at,
                width: 595.0,
                height: 842.0,
            }
        }
        Operation::AddText {
            page,
            rect,
            text: contents,
            size,
        } => {
            let info = document
                .info
                .pages
                .get(page)
                .ok_or_else(|| failure("invalid_page"))?;
            if !text(&contents)
                || contents.is_empty()
                || !size.is_finite()
                || !(6.0..=96.0).contains(&size)
                || !rect.iter().all(|number| number.is_finite())
                || rect[0] < 0.0
                || rect[1] < 0.0
                || rect[2] > f64::from(info.width)
                || rect[3] > f64::from(info.height)
                || rect[0] >= rect[2]
                || rect[1] >= rect[3]
            {
                return Err(failure("invalid_text_box"));
            }
            let added = AddedText {
                rect,
                text: contents,
                size,
                ..AddedText::default()
            };
            let fitted = pdfcraft_engine::added_text_rect(&added);
            if fitted[0] < 0.0
                || fitted[1] < 0.0
                || fitted[2] > f64::from(info.width)
                || fitted[3] > f64::from(info.height)
            {
                return Err(failure("text_would_overflow_page"));
            }
            Edit::AddText { page, text: added }
        }
        Operation::RotatePages { pages, degrees } => {
            if pages.is_empty()
                || pages.len() > count
                || !matches!(degrees, -90 | 90 | 180)
                || pages.iter().any(|page| *page >= count)
                || pages
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != pages.len()
            {
                return Err(failure("invalid_pages"));
            }
            Edit::RotatePages { pages, degrees }
        }
        Operation::ReplaceText {
            page,
            block,
            expected,
            text: replacement,
        } => {
            if page >= count || !text(&expected) || !text(&replacement) {
                return Err(failure("invalid_text"));
            }
            let blocks = document.text_blocks(page);
            if blocks
                .get(block)
                .is_none_or(|source| source.text != expected)
            {
                return Err(failure("paragraph_changed"));
            }
            Edit::EditTextBlock {
                page,
                block,
                text: replacement,
                style: BlockStyle::default(),
            }
        }
        Operation::Annotate {
            page,
            rect,
            text: contents,
        } => {
            let page_info = document
                .info
                .pages
                .get(page)
                .ok_or_else(|| failure("invalid_page"))?;
            let crop = page_info.crop.map(f64::from);
            if !text(&contents)
                || !rect.iter().all(|number| number.is_finite())
                || rect[0] < crop[0]
                || rect[1] < crop[1]
                || rect[2] > crop[2]
                || rect[3] > crop[3]
                || rect[0] >= rect[2]
                || rect[1] >= rect[3]
            {
                return Err(failure("invalid_annotation"));
            }
            let shape = Shape::Rectangle { rect };
            Edit::AddAnnotation(NewAnnotation {
                page,
                style: Style::default_for(&shape),
                shape,
                contents,
                author: author.to_owned(),
            })
        }
    };
    session
        .apply(id, edit)
        .map_err(|_| failure("engine_edit_refused"))?;
    if !session
        .get(id)
        .ok_or_else(|| failure("not_open"))?
        .edit_notices
        .is_empty()
    {
        return Err(failure("representation_loss"));
    }
    Ok(())
}
