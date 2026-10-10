//! Read-only seam owned by the relay log schema. The runtime supplies its own
//! transaction on the configured shared cluster; source heads are always read
//! with conflicts in that transaction, never copied into a runtime head key.
use foundationdb::{
    tuple::{unpack, Subspace},
    Transaction,
};
use roda_proto::Sequenced;

use crate::log::StoreError;

const MAX_BATCH: usize = 64;
const MAX_WIRE_BYTES: usize = 98_304;
const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;

pub struct RuntimeSource {
    cell: String,
    root: Subspace,
}

/// An observed relay head, not a grant or execution capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeHead {
    seq: u64,
    hash: String,
}

impl RuntimeHead {
    pub fn seq(&self) -> u64 {
        self.seq
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

pub struct RuntimeSourceBatch {
    pub head: RuntimeHead,
    pub entries: Vec<Sequenced>,
}

fn invalid() -> StoreError {
    StoreError("invalid or incomplete runtime source".into())
}

fn store_error(_: foundationdb::FdbError) -> StoreError {
    StoreError("runtime source unavailable".into())
}

impl RuntimeSource {
    /// Host configuration identifies the relay's actual cell. No arbitrary
    /// subspace, caller frontier, callback or writer is accepted here.
    pub fn configured(cell: &str) -> Result<Self, StoreError> {
        if cell.is_empty()
            || cell.len() > 128
            || !cell
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(invalid());
        }
        Ok(Self {
            cell: cell.into(),
            root: Subspace::all().subspace(&("zoen", cell)),
        })
    }

    pub fn cell(&self) -> &str {
        &self.cell
    }

    pub async fn head_in(
        &self,
        trx: &Transaction,
        space: &str,
    ) -> Result<Option<RuntimeHead>, StoreError> {
        let value = trx
            .get(&self.root.pack(&("s", space, "head")), false)
            .await
            .map_err(store_error)?;
        let Some(value) = value else { return Ok(None) };
        if value.len() > 256 {
            return Err(invalid());
        }
        let (seq, hash): (i64, String) = unpack(&value).map_err(|_| invalid())?;
        if seq < 0
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(invalid());
        }
        Ok(Some(RuntimeHead {
            seq: seq as u64,
            hash,
        }))
    }

    /// Bounded direct keys avoid partial range pages and require every ordered
    /// position. A missing/pruned unsupported entry refuses catch-up; it never
    /// silently advances the native cursor.
    pub async fn read_in(
        &self,
        trx: &Transaction,
        space: &str,
        after: Option<(u64, &str)>,
    ) -> Result<RuntimeSourceBatch, StoreError> {
        let head = self.head_in(trx, space).await?.ok_or_else(invalid)?;
        let from = match after {
            Some((seq, hash)) => {
                if seq > head.seq {
                    return Err(invalid());
                }
                let prior = self.entry_in(trx, space, seq).await?;
                if prior.hash != hash {
                    return Err(invalid());
                }
                seq.checked_add(1).ok_or_else(invalid)?
            }
            None => 0,
        };
        let mut entries = Vec::new();
        let mut bytes = 0usize;
        let end = head
            .seq
            .saturating_add(1)
            .min(from.saturating_add(MAX_BATCH as u64));
        for seq in from..end {
            let entry = self.entry_in(trx, space, seq).await?;
            bytes = bytes
                .checked_add(entry.env.content().len())
                .ok_or_else(invalid)?;
            if bytes > MAX_BATCH_BYTES {
                return Err(invalid());
            }
            entries.push(entry);
        }
        Ok(RuntimeSourceBatch { head, entries })
    }

    async fn entry_in(
        &self,
        trx: &Transaction,
        space: &str,
        seq: u64,
    ) -> Result<Sequenced, StoreError> {
        let index = i64::try_from(seq).map_err(|_| invalid())?;
        let bytes = trx
            .get(&self.root.pack(&("s", space, "log", index)), false)
            .await
            .map_err(store_error)?
            .ok_or_else(invalid)?;
        if bytes.len() > MAX_WIRE_BYTES {
            return Err(invalid());
        }
        let entry = Sequenced::decode(&bytes).map_err(|_| invalid())?;
        if entry.seq != seq || entry.env.space() != space || entry.env.content().len() > 65_536 {
            return Err(invalid());
        }
        Ok(entry)
    }
}
