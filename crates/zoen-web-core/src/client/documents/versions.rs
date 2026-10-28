//! A document always retains its authenticated creation version.
use super::Version;

#[derive(Clone)]
pub(crate) struct VersionHistory {
    first: Version,
    later: Vec<Version>,
}

impl VersionHistory {
    pub(crate) fn new(first: Version) -> Self {
        Self {
            first,
            later: Vec::new(),
        }
    }
    pub(crate) fn first(&self) -> &Version {
        &self.first
    }
    pub(crate) fn current(&self) -> &Version {
        self.later.last().unwrap_or(&self.first)
    }
    pub(crate) fn len(&self) -> usize {
        1 + self.later.len()
    }
    pub(crate) fn get(&self, index: usize) -> Option<&Version> {
        if index == 0 {
            Some(&self.first)
        } else {
            self.later.get(index - 1)
        }
    }
    pub(crate) fn push(&mut self, version: Version) {
        self.later.push(version);
    }
    pub(crate) fn truncate(&mut self, len: usize) -> Result<(), String> {
        let remaining = len
            .checked_sub(1)
            .ok_or("cannot remove authenticated document creation")?;
        self.later.truncate(remaining);
        Ok(())
    }
    pub(crate) fn iter(&self) -> Versions<'_> {
        Versions {
            first: Some(&self.first),
            later: self.later.iter(),
        }
    }
}

pub(crate) struct Versions<'a> {
    first: Option<&'a Version>,
    later: std::slice::Iter<'a, Version>,
}
impl<'a> Iterator for Versions<'a> {
    type Item = &'a Version;
    fn next(&mut self) -> Option<Self::Item> {
        self.first.take().or_else(|| self.later.next())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = usize::from(self.first.is_some()) + self.later.len();
        (len, Some(len))
    }
}
impl DoubleEndedIterator for Versions<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.later.next_back().or_else(|| self.first.take())
    }
}
impl ExactSizeIterator for Versions<'_> {}

impl<'a> IntoIterator for &'a VersionHistory {
    type Item = &'a Version;
    type IntoIter = Versions<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{Version, VersionHistory};
    use event_log::{Author, Signer};
    use types::{EventBody, ItemContent, ItemKind, PlanDoc};

    fn version(note: &str) -> Result<Version, String> {
        let content = ItemContent::Plan(PlanDoc {
            title: note.into(),
            summary: String::new(),
            budget_cents: None,
            sections: Vec::new(),
        });
        let author = Author::device(
            &Signer::from_secret(&[1; 32]),
            Signer::from_secret(&[2; 32]),
        );
        let event = author
            .sign_event(
                "space",
                "creation",
                1,
                None,
                EventBody::ItemCreated {
                    item: "item".into(),
                    kind: ItemKind::Plan,
                    content: content.clone(),
                    origin: "Created".into(),
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(Version {
            content,
            event,
            note: note.into(),
            is_undo: false,
            pending: false,
            failure: None,
        })
    }

    #[test]
    fn creation_survives_invalid_truncation_and_iteration_remains_chronological(
    ) -> Result<(), String> {
        let mut history = VersionHistory::new(version("creation")?);
        history.push(version("second")?);
        history.push(version("third")?);
        assert!(history.truncate(0).is_err());
        assert_eq!(history.len(), 3);
        assert_eq!(history.current().note, "third");
        let mut iter = history.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(
            iter.next().map(|value| value.note.as_str()),
            Some("creation")
        );
        assert_eq!(
            iter.next_back().map(|value| value.note.as_str()),
            Some("third")
        );
        assert_eq!(iter.len(), 1);
        assert_eq!(
            iter.next_back().map(|value| value.note.as_str()),
            Some("second")
        );
        assert!(iter.next().is_none());
        history.truncate(1)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history.current().note, "creation");
        assert!(std::ptr::eq(history.current(), history.first()));
        Ok(())
    }
}
