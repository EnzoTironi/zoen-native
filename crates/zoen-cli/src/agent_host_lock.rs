//! One citizen host per persistent local home. This advisory OS lock is not
//! distributed fencing, a provider-attempt ledger or a network-filesystem guarantee.
use std::fs::{File, TryLockError};
use std::path::Path;

pub(crate) struct HostLock {
    _file: File,
}

impl HostLock {
    pub(crate) fn acquire(home: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(home)
            .map_err(|error| format!("cannot create the local agent-host home: {error}"))?;
        let mut options = File::options();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Never unlink this file: all contenders must lock the same inode even
        // after a holder exits. A stale filename does not imply a held OS lock.
        let file = options
            .open(home.join("agent-host.lock"))
            .map_err(|error| format!("cannot open the local agent-host lock: {error}"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => {
                Err("another agent-host is already running for this local home".into())
            }
            Err(error) => Err(format!(
                "cannot acquire the local agent-host OS lock: {error}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::PathBuf;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "zoen-agent-host-lock-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn command(home: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("agent_host_lock_child_process")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("ZOEN_TEST_HOST_LOCK_HOME", home)
            .env("ZOEN_TEST_HOST_LOCK_MODE", mode)
            .current_dir(home)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        command
    }

    struct Holder(Child);
    impl Holder {
        fn start(home: &Path) -> Self {
            let mut holder = Self(command(home, "hold").stdin(Stdio::piped()).spawn().unwrap());
            let deadline = Instant::now() + Duration::from_secs(5);
            while !home.join("holder-ready").exists() {
                assert!(
                    holder.0.try_wait().unwrap().is_none(),
                    "holder exited before acquiring"
                );
                assert!(
                    Instant::now() < deadline,
                    "holder did not acquire the OS lock"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            holder
        }
        fn finish(&mut self) -> ExitStatus {
            // EOF releases the helper's hold without deleting the lock file.
            drop(self.0.stdin.take());
            self.0.wait().unwrap()
        }
    }
    impl Drop for Holder {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn probe(home: &Path) -> ExitStatus {
        command(home, "probe").status().unwrap()
    }

    #[test]
    fn agent_host_lock_child_process() {
        let Some(home) = std::env::var_os("ZOEN_TEST_HOST_LOCK_HOME") else {
            return;
        };
        let guard = match HostLock::acquire(Path::new(&home)) {
            Ok(guard) => guard,
            Err(error) if error == "another agent-host is already running for this local home" => {
                std::process::exit(73);
            }
            Err(error) => panic!("{error}"),
        };
        if std::env::var("ZOEN_TEST_HOST_LOCK_MODE").as_deref() == Ok("hold") {
            std::fs::write(Path::new(&home).join("holder-ready"), b"").unwrap();
            let mut byte = [0];
            assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 0);
        }
        drop(guard);
    }

    #[test]
    fn competing_process_is_refused_until_holder_exits_and_inode_is_retained() {
        let home = Home::new();
        let mut holder = Holder::start(&home.0);
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            home.0.join("agent-host.lock").metadata().unwrap().ino()
        };
        assert_eq!(probe(&home.0).code(), Some(73));
        assert_eq!(probe(&home.0).code(), Some(73));
        assert!(holder.finish().success());
        assert!(home.0.join("agent-host.lock").exists());
        assert!(probe(&home.0).success(), "exited process retained its lock");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                home.0.join("agent-host.lock").metadata().unwrap().ino(),
                inode
            );
        }
    }

    #[test]
    fn killed_process_releases_the_os_lock_without_stale_file_cleanup() {
        let home = Home::new();
        let mut holder = Holder::start(&home.0);
        assert_eq!(probe(&home.0).code(), Some(73));
        holder.0.kill().unwrap();
        assert!(!holder.0.wait().unwrap().success());
        assert!(home.0.join("agent-host.lock").exists());
        assert!(probe(&home.0).success(), "dead process retained its lock");
    }

    #[test]
    fn separate_homes_can_host_independently() {
        let first = Home::new();
        let second = Home::new();
        let _holder = Holder::start(&first.0);
        assert!(probe(&second.0).success());
        assert_eq!(probe(&first.0).code(), Some(73));
    }

    #[test]
    fn dropping_guard_releases_lock_while_process_stays_alive() {
        let home = Home::new();
        let guard = HostLock::acquire(&home.0).unwrap();
        assert_eq!(probe(&home.0).code(), Some(73));
        drop(guard);
        assert!(probe(&home.0).success());
    }

    #[test]
    fn unavailable_home_or_lock_file_fails_closed() {
        let home = Home::new();
        let file = home.0.join("not-a-directory");
        File::create(&file).unwrap();
        assert!(HostLock::acquire(&file)
            .err()
            .unwrap()
            .starts_with("cannot create"));
        std::fs::create_dir(home.0.join("agent-host.lock")).unwrap();
        assert!(HostLock::acquire(&home.0)
            .err()
            .unwrap()
            .starts_with("cannot open"));
    }
}
