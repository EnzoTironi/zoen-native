//! Local CLI secrets. Each replacement is atomic; a failed acknowledgement may
//! already have committed the complete new value. This is not a multi-key
//! transaction or a substitute for the enrollment/setup recovery record. The
//! CLI establishes its home before opening this adapter; only the vault's entry
//! within that existing home is created and synchronized here.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const TEMP_PREFIX: &str = ".vault-write.";

pub(crate) struct FileVault {
    pub(crate) dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveStage {
    DirectoryReady,
    TemporaryCreated,
    BytesWritten,
    FileSynced,
    Renamed,
    DirectorySynced,
}

struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            // Only this writer's uncommitted temporary file. Orphans left by
            // process death are never mistaken for keys or deleted on startup.
            let _ = fs::remove_file(path);
        }
    }
}

fn invalid_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid local vault path")
}

fn validate_key(key: &str) -> io::Result<()> {
    if key.is_empty()
        || key.len() > 128
        || key == "."
        || key == ".."
        || key.starts_with(TEMP_PREFIX)
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid_path());
    }
    Ok(())
}

fn require_regular_or_missing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_path()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        // std does not expose a portable directory-flush primitive. File sync
        // and atomic replacement still apply; directory durability is currently
        // qualified only on the tested Unix host, not on Windows/network shares.
        let _ = path;
        Ok(())
    }
}

impl FileVault {
    pub(crate) fn read_bytes(&self, key: &str) -> io::Result<Vec<u8>> {
        validate_key(key)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        fs::read(path)
    }

    pub(crate) fn write_bytes(&self, key: &str, value: &[u8]) -> io::Result<()> {
        self.write_observed(key, value, |_| Ok(()))
    }

    // The private observer lets tests interrupt the actual filesystem path at
    // commit boundaries. Production has only the no-op observer above.
    fn write_observed(
        &self,
        key: &str,
        value: &[u8],
        mut after: impl FnMut(SaveStage) -> io::Result<()>,
    ) -> io::Result<()> {
        validate_key(key)?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&self.dir) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
        if !fs::symlink_metadata(&self.dir)?.is_dir() {
            return Err(invalid_path());
        }
        // Sync even when the vault exists: its creation may have been interrupted
        // before this acknowledgement. Do not require access to unrelated system
        // ancestors; establishing the parent home belongs to the CLI caller.
        let parent = self
            .dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        sync_directory(parent)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        after(SaveStage::DirectoryReady)?;

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let (mut file, temporary) = (0..128)
            .find_map(|_| {
                let temporary = self.dir.join(format!(
                    "{TEMP_PREFIX}{}.{timestamp}.{}.tmp",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match options.open(&temporary) {
                    Ok(file) => Some(Ok((file, temporary))),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .unwrap_or_else(|| Err(io::Error::other("local vault temporary path unavailable")))?;
        let mut guard = Temporary(Some(temporary.clone()));
        after(SaveStage::TemporaryCreated)?;
        file.write_all(value)?;
        after(SaveStage::BytesWritten)?;
        file.sync_all()?;
        after(SaveStage::FileSynced)?;
        drop(file);
        require_regular_or_missing(&path)?;
        fs::rename(&temporary, &path)?;
        guard.0 = None;
        // Do not roll back or delete the destination on any error after rename.
        // A caller receiving false must reconcile the complete retained value.
        after(SaveStage::Renamed)?;
        sync_directory(&self.dir)?;
        after(SaveStage::DirectorySynced)?;
        Ok(())
    }

    pub(crate) fn remove(&self, key: &str) -> io::Result<()> {
        validate_key(key)?;
        let path = self.dir.join(key);
        require_regular_or_missing(&path)?;
        fs::remove_file(path)?;
        sync_directory(&self.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "zoen-file-vault-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&path).unwrap();
            Self(path)
        }
        fn vault(&self) -> FileVault {
            FileVault {
                dir: self.0.join("vault"),
            }
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temporary_count(vault: &FileVault) -> usize {
        fs::read_dir(&vault.dir)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(TEMP_PREFIX)
            })
            .count()
    }

    #[test]
    fn replacement_preserves_full_binary_values_and_private_permissions() {
        let home = Home::new();
        let vault = home.vault();
        vault
            .write_bytes("zoen.identity.v1", &[0, 255, 42])
            .unwrap();
        assert_eq!(vault.read_bytes("zoen.identity.v1").unwrap(), [0, 255, 42]);
        let replacement = vec![73; 131_073];
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                vault.dir.join("zoen.identity.v1"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }
        vault.write_bytes("zoen.identity.v1", &replacement).unwrap();
        assert_eq!(vault.read_bytes("zoen.identity.v1").unwrap(), replacement);
        assert_eq!(temporary_count(&vault), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&vault.dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(vault.dir.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(vault.dir.join("zoen.identity.v1"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        vault.remove("zoen.identity.v1").unwrap();
        assert!(vault.read_bytes("zoen.identity.v1").is_err());
    }

    #[test]
    fn invalid_keys_cannot_read_write_or_remove_outside_the_vault() {
        let home = Home::new();
        let vault = home.vault();
        let sentinel = home.0.join("sentinel");
        fs::write(&sentinel, b"keep").unwrap();
        for key in [
            "",
            ".",
            "..",
            "../sentinel",
            "/sentinel",
            "a/b",
            "a\\b",
            "a:stream",
            "a\0b",
            ".vault-write.orphan",
            &"x".repeat(129),
        ] {
            assert!(vault.read_bytes(key).is_err());
            assert!(vault.write_bytes(key, b"replace").is_err());
            assert!(vault.remove(key).is_err());
        }
        assert!(!vault.dir.exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"keep");
    }

    #[test]
    fn precommit_failures_retain_old_bytes_and_clean_only_the_owned_temporary() {
        for point in [
            SaveStage::DirectoryReady,
            SaveStage::TemporaryCreated,
            SaveStage::BytesWritten,
            SaveStage::FileSynced,
        ] {
            let home = Home::new();
            let vault = home.vault();
            vault.write_bytes("agent_root", b"old").unwrap();
            let orphan = vault.dir.join(format!("{TEMP_PREFIX}other-writer.tmp"));
            fs::write(&orphan, b"other").unwrap();
            let result = vault.write_observed("agent_root", &vec![31; 65_537], |stage| {
                if stage == point {
                    Err(io::Error::other("injected interruption"))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err(), "{point:?}");
            assert_eq!(vault.read_bytes("agent_root").unwrap(), b"old", "{point:?}");
            assert_eq!(temporary_count(&vault), 1);
            assert_eq!(fs::read(orphan).unwrap(), b"other");
        }
    }

    #[test]
    fn lost_acknowledgement_after_rename_keeps_the_new_complete_value() {
        for point in [SaveStage::Renamed, SaveStage::DirectorySynced] {
            let home = Home::new();
            let vault = home.vault();
            vault.write_bytes("agent_root", b"old").unwrap();
            let new = vec![19; 65_537];
            assert!(vault
                .write_observed("agent_root", &new, |stage| {
                    if stage == point {
                        Err(io::Error::other("lost acknowledgement"))
                    } else {
                        Ok(())
                    }
                })
                .is_err());
            assert_eq!(vault.read_bytes("agent_root").unwrap(), new);
            assert_eq!(temporary_count(&vault), 0);
        }
    }

    #[test]
    fn concurrent_replacements_never_expose_partial_values() {
        let home = Home::new();
        let vault = home.vault();
        let value = vec![0; 32_769];
        vault.write_bytes("same_key", &value).unwrap();
        std::thread::scope(|scope| {
            let (ready, waiting) = std::sync::mpsc::channel();
            let mut releases = Vec::new();
            for id in 1..=8 {
                let vault = &vault;
                let ready = ready.clone();
                let (release, proceed) = std::sync::mpsc::channel();
                releases.push(release);
                scope.spawn(move || {
                    for round in 0..4 {
                        vault
                            .write_observed("same_key", &vec![id; 32_769], |stage| {
                                if round == 0 && stage == SaveStage::TemporaryCreated {
                                    ready.send(()).map_err(io::Error::other)?;
                                    proceed
                                        .recv_timeout(Duration::from_secs(5))
                                        .map_err(io::Error::other)?;
                                }
                                Ok(())
                            })
                            .unwrap();
                        let bytes = vault.read_bytes("same_key").unwrap();
                        assert_eq!(bytes.len(), 32_769);
                        assert!(bytes.iter().all(|byte| *byte == bytes[0]));
                    }
                });
            }
            drop(ready);
            for _ in 0..8 {
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            let simultaneous = temporary_count(&vault);
            for release in releases {
                let _ = release.send(());
            }
            assert_eq!(simultaneous, 8);
        });
        assert_eq!(temporary_count(&vault), 0);
    }

    #[test]
    fn non_regular_destinations_are_refused_without_modifying_them() {
        let home = Home::new();
        let vault = home.vault();
        vault.write_bytes("regular", b"keep").unwrap();
        fs::create_dir(vault.dir.join("directory")).unwrap();
        assert!(vault.write_bytes("directory", b"new").is_err());
        assert!(vault.remove("directory").is_err());
        assert!(vault.dir.join("directory").is_dir());
        #[cfg(unix)]
        {
            let outside = home.0.join("outside");
            fs::write(&outside, b"keep").unwrap();
            std::os::unix::fs::symlink(&outside, vault.dir.join("symlink")).unwrap();
            assert!(vault.read_bytes("symlink").is_err());
            assert!(vault.write_bytes("symlink", b"new").is_err());
            assert!(vault.remove("symlink").is_err());
            assert_eq!(fs::read(outside).unwrap(), b"keep");
        }
    }

    #[test]
    fn rename_io_error_preserves_the_old_key() {
        let home = Home::new();
        let vault = home.vault();
        vault.write_bytes("agent_root", b"old").unwrap();
        let result = vault.write_observed("agent_root", b"new", |stage| {
            if stage == SaveStage::FileSynced {
                let temporary = fs::read_dir(&vault.dir)
                    .unwrap()
                    .map(Result::unwrap)
                    .find(|entry| entry.file_name().to_string_lossy().starts_with(TEMP_PREFIX))
                    .unwrap();
                // Actual ENOENT from rename, rather than a mocked rename result.
                fs::remove_file(temporary.path()).unwrap();
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(vault.read_bytes("agent_root").unwrap(), b"old");
        assert_eq!(temporary_count(&vault), 0);
    }

    #[test]
    fn missing_parent_home_is_refused_without_creating_an_unowned_tree() {
        let home = Home::new();
        let vault = FileVault {
            dir: home.0.join("missing-home/vault"),
        };
        assert!(vault.write_bytes("agent_root", b"key").is_err());
        assert!(!home.0.join("missing-home").exists());
    }

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn file_vault_interrupted_child() {
        let Some(home) = std::env::var_os("ZOEN_TEST_FILE_VAULT_HOME") else {
            return;
        };
        let vault = FileVault {
            dir: PathBuf::from(&home).join("vault"),
        };
        if std::env::var("ZOEN_TEST_FILE_VAULT_MODE").as_deref() == Ok("limited-write") {
            let error = vault
                .write_bytes("agent_root", &vec![29; 131_073])
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
            assert_eq!(vault.read_bytes("agent_root").unwrap(), b"old");
            assert_eq!(temporary_count(&vault), 0);
            return;
        }
        let point = std::env::var("ZOEN_TEST_FILE_VAULT_STAGE").unwrap();
        vault
            .write_observed("agent_root", &vec![27; 131_073], |stage| {
                if format!("{stage:?}") == point {
                    fs::write(Path::new(&home).join("child-ready"), b"").unwrap();
                    loop {
                        std::thread::park();
                    }
                }
                Ok(())
            })
            .unwrap();
        panic!("child failed to stop at its commit boundary");
    }

    #[test]
    fn killed_process_before_and_after_rename_recovers_complete_keys() {
        for point in [
            SaveStage::TemporaryCreated,
            SaveStage::BytesWritten,
            SaveStage::FileSynced,
            SaveStage::Renamed,
            SaveStage::DirectorySynced,
        ] {
            let home = Home::new();
            let vault = home.vault();
            vault.write_bytes("agent_root", b"old").unwrap();
            let mut child = ChildGuard(
                Command::new(std::env::current_exe().unwrap())
                    .arg("file_vault_interrupted_child")
                    .arg("--nocapture")
                    .arg("--test-threads=1")
                    .env("ZOEN_TEST_FILE_VAULT_HOME", &home.0)
                    .env("ZOEN_TEST_FILE_VAULT_STAGE", format!("{point:?}"))
                    .current_dir(&home.0)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            while !home.0.join("child-ready").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited at {point:?}"
                );
                assert!(Instant::now() < deadline, "child did not reach {point:?}");
                std::thread::sleep(Duration::from_millis(10));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let recovered = home.vault();
            if matches!(point, SaveStage::Renamed | SaveStage::DirectorySynced) {
                assert_eq!(
                    recovered.read_bytes("agent_root").unwrap(),
                    vec![27; 131_073]
                );
                assert_eq!(temporary_count(&recovered), 0);
            } else {
                assert_eq!(recovered.read_bytes("agent_root").unwrap(), b"old");
                assert_eq!(temporary_count(&recovered), 1);
            }
            recovered.write_bytes("agent_root", b"recovered").unwrap();
            assert_eq!(recovered.read_bytes("agent_root").unwrap(), b"recovered");
        }
    }

    #[cfg(unix)]
    #[test]
    fn actual_partial_write_failure_keeps_the_old_key_and_cleans_the_temporary() {
        let home = Home::new();
        let vault = home.vault();
        vault.write_bytes("agent_root", b"old").unwrap();
        let result = Command::new("/bin/sh")
            .arg("-c")
            .arg("ulimit -f 2 || exit 97; trap '' XFSZ; exec \"$1\" file_vault_interrupted_child --nocapture --test-threads=1")
            .arg("file-vault-limited-write")
            .arg(std::env::current_exe().unwrap())
            .env("ZOEN_TEST_FILE_VAULT_HOME", &home.0)
            .env("ZOEN_TEST_FILE_VAULT_MODE", "limited-write")
            .current_dir(&home.0)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(vault.read_bytes("agent_root").unwrap(), b"old");
        assert_eq!(temporary_count(&vault), 0);
    }
}
