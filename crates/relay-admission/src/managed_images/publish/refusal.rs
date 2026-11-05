//! A committed negative outcome updates the original independent consumption.
//! It creates neither another append permit nor financial release evidence.
use super::*;
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishRefusal {
    SourceChanged,
    Expired,
    Cancelled,
    Closed,
}
impl PublishRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::SourceChanged => "managed authorization source changed",
            Self::Expired => "managed authorization expired",
            Self::Cancelled => "managed run cancelled",
            Self::Closed => "managed run closed",
        }
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl AnchorReader {
    /// Original terminal evidence only. A missing consumption is not refusal.
    pub fn publication_refusal(
        &self,
        checkpoint: &Checkpoint,
        envelope: &Envelope,
    ) -> Result<Option<PublishRefusal>, AnchorError> {
        let _generation = self.effect_generation_guard(checkpoint)?;
        let expected = self.publish_record(checkpoint, envelope)?.condition()?;
        let path = self
            .effect_directory()?
            .join(format!("{}.json", expected.effect_digest));
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(unavailable(error)),
            Ok(_) => {}
        }
        let record = self.read_publish(&expected.effect_digest)?;
        if record.condition()? != expected {
            return Err(AnchorError::Unreconciled);
        }
        if record.refusal.is_some() {
            sync_directory(&self.effect_directory()?)?;
        }
        Ok(record.refusal)
    }
}

impl ManagedPublishPermit {
    /// Only the linear queue owner, after a known committed rejection with no
    /// prior maybe-committed attempt, may call this. Failure preserves Unknown.
    pub fn record_committed_refusal(&self, refusal: PublishRefusal) -> Result<(), AnchorError> {
        let mut record = self.reader.read_publish(&self.condition.effect_digest)?;
        if record.condition()? != self.condition {
            return Err(AnchorError::Unreconciled);
        }
        if let Some(original) = record.refusal {
            return if original == refusal {
                sync_directory(&self.reader.effect_directory()?)
            } else {
                Err(AnchorError::Unreconciled)
            };
        }
        let _generation = self
            .reader
            .effect_generation_guard(&self.condition.checkpoint)?;
        record.refusal = Some(refusal);
        let bytes = serde_json::to_vec(&record).map_err(|_| AnchorError::Invalid)?;
        let directory = self.reader.effect_directory()?;
        let suffix = types::new_id("refusal").map_err(|_| AnchorError::Unavailable)?;
        let temporary = Temporary(directory.join(format!(".{suffix}.tmp")));
        let mut file = options().open(&temporary.0).map_err(unavailable)?;
        #[cfg(test)]
        super::super::faults::check(super::super::faults::Point::Write)?;
        file.write_all(&bytes).map_err(unavailable)?;
        #[cfg(test)]
        super::super::faults::check(super::super::faults::Point::FileSync)?;
        file.sync_all().map_err(unavailable)?;
        fs::rename(
            &temporary.0,
            directory.join(format!("{}.json", record.effect_digest)),
        )
        .map_err(unavailable)?;
        #[cfg(test)]
        super::super::faults::check(super::super::faults::Point::DirectorySync)?;
        sync_directory(&directory)
    }
}
