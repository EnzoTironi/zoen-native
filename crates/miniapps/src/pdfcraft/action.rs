//! Closed intent wire shared verbatim with the actual engine bridge.
//! Structural intent only; the engine owns page/text/geometry semantics.
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
