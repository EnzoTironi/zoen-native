//! Bounded scheduling hints from the relay's existing membership index.
use super::RuntimeSource;
/// Distinguish unavailable storage from malformed scheduling state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMembershipError {
    Invalid,
    Unavailable,
}
use foundationdb::{KeySelector, RangeOption, Transaction};

impl RuntimeSource {
    /// Membership is a scheduling hint, never conversation/grant authority.
    pub async fn membership_page_in(
        &self,
        tx: &Transaction,
        identity: &str,
        after: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), RuntimeMembershipError> {
        let valid = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        };
        if !valid(identity) || after.is_some_and(|value| !valid(value)) {
            return Err(RuntimeMembershipError::Invalid);
        }
        let prefix = self.root.subspace(&("i", identity));
        let mut range = RangeOption::from(prefix.range());
        if let Some(after) = after {
            range.begin = KeySelector::first_greater_than(prefix.pack(&(after,)));
        }
        range.limit = Some(32);
        range.target_bytes = 8192;
        let rows = tx
            .get_range(&range, 1, false)
            .await
            .map_err(|_| RuntimeMembershipError::Unavailable)?;
        let mut spaces = Vec::new();
        for row in rows.iter() {
            let (space,): (String,) = prefix
                .unpack(row.key())
                .map_err(|_| RuntimeMembershipError::Invalid)?;
            if !valid(&space) {
                return Err(RuntimeMembershipError::Invalid);
            }
            spaces.push(space);
        }
        let next = if rows.more() {
            Some(
                spaces
                    .last()
                    .ok_or(RuntimeMembershipError::Invalid)?
                    .clone(),
            )
        } else {
            None
        };
        Ok((spaces, next))
    }
}
