use super::*;
impl FdbLog {
    pub(super) async fn membership_read_version_impl(&self) -> Result<i64, StoreError> {
        let trx = self
            .cell
            .db
            .create_trx()
            .map_err(|e| StoreError(e.to_string()))?;
        trx.get_read_version()
            .await
            .map_err(|e| StoreError(e.to_string()))
    }
    pub(super) async fn membership_page_impl(
        &self,
        who: &str,
        version: i64,
        after: &str,
        limit: usize,
    ) -> Result<(Vec<String>, bool), StoreError> {
        use protocol::complete_reads as reads;
        if version <= 0 || limit == 0 || limit > reads::PAGE_ROWS || after.len() > reads::KEY_BYTES
        {
            return Err(StoreError(reads::INVALID.into()));
        }
        let trx = self
            .cell
            .db
            .create_trx()
            .map_err(|e| StoreError(e.to_string()))?;
        trx.set_read_version(version);
        let range = self.cell.root.subspace(&("i", who));
        let mut opt = RangeOption::from(range.range());
        if !after.is_empty() {
            opt.begin = foundationdb::KeySelector::first_greater_than(range.pack(&(after,)));
        }
        opt.limit = Some(limit);
        opt.target_bytes = 64 * 1024;
        opt.mode = StreamingMode::WantAll;
        let page = trx
            .get_range(&opt, 1, true)
            .await
            .map_err(|e| StoreError(e.to_string()))?;
        let mut spaces = Vec::new();
        for row in page.iter() {
            let (space,) = range
                .unpack::<(String,)>(row.key())
                .map_err(|_| StoreError(reads::INVALID.into()))?;
            if space.is_empty() || space.len() > reads::KEY_BYTES {
                return Err(StoreError(reads::PROFILE_EXCEEDED.into()));
            }
            spaces.push(space);
        }
        if page.more() && spaces.is_empty() {
            return Err(StoreError(reads::INVALID.into()));
        }
        Ok((spaces, !page.more()))
    }
}
