//! Thread-local, one-shot I/O failures at real ownership/durability boundaries.
use super::AnchorError;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Point {
    Write,
    FileSync,
    DirectorySync,
    Unlock,
}

thread_local! {
    static NEXT: Cell<Option<Point>> = const { Cell::new(None) };
}

pub(super) fn fail(point: Point) {
    NEXT.with(|next| next.set(Some(point)));
}

pub(super) fn check(point: Point) -> Result<(), AnchorError> {
    NEXT.with(|next| {
        if next.get() == Some(point) {
            next.set(None);
            Err(AnchorError::Unavailable)
        } else {
            Ok(())
        }
    })
}
