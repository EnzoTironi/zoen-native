//! Bounded opaque originals staged under the existing independent writer.
//! Bytes and content hashes are inert evidence, never execution permission.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::{
    file::{options, regular, sync_directory, unavailable},
    state::digest,
    AnchorError, AnchorReader, AnchorWriter,
};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};
const MAXIMUM: usize = 512 * 1024;
const MAX_FILES: usize = 512;
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
impl AnchorReader {
    fn originals_directory(&self) -> Result<Option<PathBuf>, AnchorError> {
        let path = self.directory.join("originals");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(unavailable(error)),
            Ok(_) => self.private_directory("originals").map(Some),
        }
    }
    /// List retained content addresses without creating missing storage.
    ///
    /// # Errors
    /// Refuses corrupt/private-file failures and the bounded profile's quota.
    pub fn retained_originals(&self) -> Result<Vec<String>, AnchorError> {
        self.state()?;
        let Some(directory) = self.originals_directory()? else {
            return Ok(Vec::new());
        };
        let mut keys = Vec::new();
        for entry in fs::read_dir(directory).map_err(unavailable)? {
            let entry = entry.map_err(unavailable)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| AnchorError::Invalid)?;
            if name.starts_with(".original-")
                && std::path::Path::new(&name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
            {
                continue;
            }
            let key = name
                .strip_suffix(".bin")
                .filter(|value| digest(value))
                .ok_or(AnchorError::Invalid)?;
            regular(&entry.path())?;
            keys.push(key.to_owned());
            if keys.len() > MAX_FILES {
                return Err(AnchorError::Invalid);
            }
        }
        keys.sort();
        Ok(keys)
    }
    /// Read the exact original and authenticate its content address.
    ///
    /// # Errors
    /// Missing, oversized, non-private, non-regular or changed bytes refuse.
    pub fn retained_original(&self, key: &str) -> Result<Vec<u8>, AnchorError> {
        self.retained_original_bounded(key, MAXIMUM)
    }

    /// Read a content-addressed original under an explicit host memory bound.
    ///
    /// # Errors
    /// Refuses invalid bounds, changed bytes and the same storage failures as
    /// `retained_original`; it never raises the default reader's bound.
    pub fn retained_original_bounded(
        &self,
        key: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, AnchorError> {
        if maximum == 0 || maximum > 64 * 1024 * 1024 {
            return Err(AnchorError::Invalid);
        }
        self.state()?;
        if !digest(key) {
            return Err(AnchorError::Invalid);
        }
        let directory = self
            .originals_directory()?
            .ok_or(AnchorError::Unreconciled)?;
        let path = directory.join(format!("{key}.bin"));
        regular(&path)?;
        let file = fs::File::open(path).map_err(unavailable)?;
        let length = file.metadata().map_err(unavailable)?.len();
        if length == 0 || length > u64::try_from(maximum).map_err(|_| AnchorError::Invalid)? {
            return Err(AnchorError::Invalid);
        }
        let mut bytes = Vec::new();
        file.take(u64::try_from(maximum + 1).map_err(|_| AnchorError::Invalid)?)
            .read_to_end(&mut bytes)
            .map_err(unavailable)?;
        if bytes.len() > maximum || event_log::sha256_hex(&bytes) != key {
            return Err(AnchorError::Unreconciled);
        }
        Ok(bytes)
    }
}
impl AnchorWriter {
    /// Durably retain an opaque original before SQL/FDB work. This cannot move
    /// a checkpoint, consume a claim, charge money or authorize a provider.
    ///
    /// # Errors
    /// Refuses quota, corruption and unknown write acknowledgements. Retry the
    /// same bytes; an existing exact original is synchronized, never replaced.
    pub fn retain_original(&mut self, bytes: &[u8]) -> Result<String, AnchorError> {
        self.retain_bounded_original(bytes, MAXIMUM)
    }

    /// Retain a larger opaque original with the same atomic writer and a
    /// caller-declared bound; the default API remains limited to 512 KiB.
    ///
    /// # Errors
    /// Refuses invalid bounds, quota, corruption and unknown acknowledgements.
    pub fn retain_bounded_original(
        &mut self,
        bytes: &[u8],
        maximum: usize,
    ) -> Result<String, AnchorError> {
        if maximum == 0 || maximum > 64 * 1024 * 1024 {
            return Err(AnchorError::Invalid);
        }
        self.reader().state()?;
        if bytes.is_empty() || bytes.len() > maximum {
            return Err(AnchorError::Invalid);
        }
        let key = event_log::sha256_hex(bytes);
        let keys = self.reader().retained_originals()?;
        if keys.contains(&key) {
            if self.reader().retained_original_bounded(&key, maximum)? != bytes {
                return Err(AnchorError::Unreconciled);
            }
            let directory = self
                .reader()
                .originals_directory()?
                .ok_or(AnchorError::Unreconciled)?;
            fs::File::open(directory.join(format!("{key}.bin")))
                .and_then(|file| file.sync_all())
                .map_err(unavailable)?;
            sync_directory(&directory)?;
            return Ok(key);
        }
        if keys.len() >= MAX_FILES {
            return Err(AnchorError::Invalid);
        }
        if self.reader().originals_directory()?.is_none() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(self.reader().directory.join("originals"))
                .map_err(unavailable)?;
            sync_directory(&self.reader().directory)?;
        }
        let directory = self
            .reader()
            .originals_directory()?
            .ok_or(AnchorError::Unreconciled)?;
        let suffix = types::new_id("original").map_err(|_| AnchorError::Unavailable)?;
        let temporary = Temporary(directory.join(format!(".original-{suffix}.tmp")));
        let mut file = options().open(&temporary.0).map_err(unavailable)?;
        #[cfg(test)]
        super::faults::check(super::faults::Point::Write)?;
        file.write_all(bytes).map_err(unavailable)?;
        #[cfg(test)]
        super::faults::check(super::faults::Point::FileSync)?;
        file.sync_all().map_err(unavailable)?;
        fs::rename(&temporary.0, directory.join(format!("{key}.bin"))).map_err(unavailable)?;
        #[cfg(test)]
        super::faults::check(super::faults::Point::DirectorySync)?;
        sync_directory(&directory)?;
        Ok(key)
    }
}
