//! One initial public-package outbox inside the complete native device image.
//! Retention is not relay publication authority or acknowledgement evidence.
use super::*;

const KEY: &str = "runtime.initial-key-packages/1";
const COUNT: usize = 32;
const MAX_RECORD: usize = 512 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u8,
    device: String,
    packages: Vec<Vec<u8>>,
    acknowledged: bool,
}

/// Public MLS material only. Constructed by the facade, not an admission permit.
pub struct InitialKeyPackages {
    packages: Vec<Vec<u8>>,
    digest: String,
    acknowledged: bool,
}
impl InitialKeyPackages {
    pub fn packages(&self) -> &[Vec<u8>] {
        &self.packages
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn acknowledged(&self) -> bool {
        self.acknowledged
    }
}

impl DeviceCore {
    fn initial_record(&self) -> Result<Option<Record>> {
        if !self.usable {
            return Err(NativeError::History);
        }
        let Some(bytes) = self
            .engine
            .store
            .meta(KEY)
            .map_err(|_| NativeError::Storage)?
        else {
            return Ok(None);
        };
        if bytes.len() > MAX_RECORD {
            return Err(NativeError::Bounds);
        }
        let record: Record = serde_json::from_str(&bytes).map_err(|_| NativeError::Image)?;
        let device = self
            .engine
            .net
            .account
            .as_ref()
            .ok_or(NativeError::Credential)?;
        if record.format != 1
            || record.device != device.device
            || record.packages.len() != COUNT
            || record
                .packages
                .iter()
                .any(|p| p.is_empty() || p.len() > 4096)
        {
            return Err(NativeError::Image);
        }
        Ok(Some(record))
    }
    fn save_initial_packages(&self, record: &Record) -> Result<()> {
        let bytes = serde_json::to_string(record).map_err(|_| NativeError::Storage)?;
        if bytes.len() > MAX_RECORD {
            return Err(NativeError::Bounds);
        }
        self.engine
            .store
            .set_meta(KEY, &bytes)
            .map_err(|_| NativeError::Storage)
    }
    /// Reads an inert publication record from this image; it is not proof of
    /// current relay stock, SQL enrollment or a live publication capability.
    pub fn initial_key_package_publication(&self) -> Result<Option<InitialKeyPackages>> {
        self.initial_record()?
            .map(|record| {
                let digest = event_log::sha256_hex(
                    &serde_json::to_vec(&record.packages).map_err(|_| NativeError::Storage)?,
                );
                Ok(InitialKeyPackages {
                    packages: record.packages,
                    digest,
                    acknowledged: record.acknowledged,
                })
            })
            .transpose()
    }
    /// Generates only if this initial outbox has never existed. The returned
    /// image must be activated before the host publishes any returned bytes.
    /// Reopen/retry returns the exact same bytes, including after consumption.
    pub fn prepare_initial_key_packages(&mut self) -> Result<(InitialKeyPackages, NativeImage)> {
        let result = (|| {
            let record = match self.initial_record()? {
                Some(record) => record,
                None => {
                    let (packages, _) = self.key_packages(COUNT)?;
                    let record = Record {
                        format: 1,
                        device: self
                            .engine
                            .net
                            .account
                            .as_ref()
                            .ok_or(NativeError::Credential)?
                            .device
                            .clone(),
                        packages,
                        acknowledged: false,
                    };
                    self.save_initial_packages(&record)?;
                    record
                }
            };
            let digest = event_log::sha256_hex(
                &serde_json::to_vec(&record.packages).map_err(|_| NativeError::Storage)?,
            );
            Ok((
                InitialKeyPackages {
                    packages: record.packages,
                    digest,
                    acknowledged: record.acknowledged,
                },
                self.image()?,
            ))
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }
    /// Records a known host publication acknowledgement for this exact pending
    /// batch. It performs no network action and creates no replacement package.
    pub fn acknowledge_initial_key_packages(&mut self, digest: &str) -> Result<NativeImage> {
        let result = (|| {
            let mut record = self.initial_record()?.ok_or(NativeError::History)?;
            let expected = event_log::sha256_hex(
                &serde_json::to_vec(&record.packages).map_err(|_| NativeError::Storage)?,
            );
            if expected != digest {
                return Err(NativeError::History);
            }
            record.acknowledged = true;
            self.save_initial_packages(&record)?;
            self.image()
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }
}
