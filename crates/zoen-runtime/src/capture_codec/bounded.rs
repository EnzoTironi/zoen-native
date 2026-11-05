//! Bound serialization before allocation, including hostile/large tool output.
use crate::RuntimeError;
use std::io::{self, Write};
pub(super) fn json(value: &impl serde::Serialize, limit: usize) -> Result<Vec<u8>, RuntimeError> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::Error::other("capture byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| RuntimeError::EvidenceTooLarge)?;
    Ok(writer.bytes)
}
