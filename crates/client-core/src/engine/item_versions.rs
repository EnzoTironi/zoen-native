//! An accepted Item always owns its initial version and any later updates.
//! This is the same replay projection; no extra checkpoint or history authority.
use super::Version;

#[derive(Clone, Debug)]
pub(crate) struct ItemVersions {
    pub(crate) initial: Version,
    pub(crate) updates: Vec<Version>,
}

impl ItemVersions {
    pub(crate) fn new(initial: Version) -> Self {
        Self {
            initial,
            updates: Vec::new(),
        }
    }

    pub(crate) fn initial(&self) -> &Version {
        &self.initial
    }

    pub(crate) fn current(&self) -> &Version {
        self.updates.last().unwrap_or(&self.initial)
    }

    pub(crate) fn len(&self) -> usize {
        1 + self.updates.len()
    }

    pub(crate) fn get(&self, index: usize) -> Option<&Version> {
        match index {
            0 => Some(&self.initial),
            other => self.updates.get(other - 1),
        }
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Version> {
        self.into_iter()
    }

    pub(crate) fn push(&mut self, version: Version) {
        self.updates.push(version);
    }

    /// Keep the initial version and updates through the selected zero-based index.
    pub(crate) fn truncate_after(&mut self, index: usize) {
        self.updates.truncate(index);
    }
}

impl<'a> IntoIterator for &'a ItemVersions {
    type Item = &'a Version;
    type IntoIter = std::iter::Chain<std::iter::Once<&'a Version>, std::slice::Iter<'a, Version>>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(&self.initial).chain(self.updates.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::{ItemVersions, Version};
    use types::ItemContent;

    fn version(number: u32) -> Version {
        Version {
            number,
            content: ItemContent::Text {
                text: format!("version {number}"),
            },
            author: "owner".into(),
            at_ms: 0,
            note: "retained".into(),
            is_undo: false,
        }
    }

    #[test]
    fn selected_history_never_drops_its_original_or_confuses_current_with_initial() {
        let mut versions = ItemVersions::new(version(1));
        versions.push(version(2));
        versions.push(version(3));
        assert_eq!(
            versions.iter().map(|v| v.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            versions.iter().rev().map(|v| v.number).collect::<Vec<_>>(),
            [3, 2, 1]
        );
        versions.truncate_after(1);
        assert_eq!(versions.current().number, 2);
        assert_eq!(versions.initial().number, 1);
        assert_eq!(versions.len(), 2);
        assert!(versions.get(2).is_none());
        versions.truncate_after(usize::MAX);
        assert_eq!(versions.current().number, 2);
        versions.truncate_after(0);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions.current().number, 1);
        assert!(versions.get(1).is_none());
    }
}
