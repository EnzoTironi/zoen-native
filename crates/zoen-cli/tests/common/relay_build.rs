//! Keep the package-only relay build separate from the enclosing workspace build.
use std::{path::Path, process::Command};

pub fn configure(command: &mut Command, manifest: &Path, target: &Path) {
    command
        .args(["build", "--locked", "-q", "-p", "zoen-relay"])
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target)
        .env("CARGO_BUILD_BUILD_DIR", target);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, time::SystemTime};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("fixture clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "zoen-relay-build-directory-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("new owned fixture");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove owned fixture");
        }
    }

    #[test]
    fn inherited_build_directory_cannot_escape_the_relay_target() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        let outer = fixture.0.join("outer-intermediates");
        let target = fixture.0.join("relay-target");
        fs::create_dir_all(source.join("src")).expect("fixture source");
        fs::create_dir(&outer).expect("outer fixture directory");
        fs::write(outer.join("canary"), b"outer compilation remains untouched")
            .expect("outer canary");
        let manifest = source.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = 'zoen-relay'\nversion = '0.0.0'\nedition = '2021'\n\n[workspace]\n",
        )
        .expect("fixture manifest");
        fs::write(source.join("src/main.rs"), "fn main() {}\n").expect("fixture main");
        let cargo = std::env::var_os("CARGO").expect("real Cargo path");
        let lock = Command::new(&cargo)
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest)
            .output()
            .expect("generate fixture lockfile");
        assert!(lock.status.success(), "{lock:?}");

        // Simulate the enclosing source guard's build-directory override without
        // mutating process-global environment or touching its real target.
        let mut command = Command::new(&cargo);
        command
            .env("CARGO_BUILD_BUILD_DIR", &outer)
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_BUILD_JOBS", "1")
            .current_dir(&source);
        configure(&mut command, &manifest, &target);
        let output = command.output().expect("real nested Cargo build");
        assert!(output.status.success(), "{output:?}");

        let mut outer_entries: Vec<_> = fs::read_dir(&outer)
            .expect("outer directory")
            .map(|entry| entry.expect("outer entry").file_name())
            .collect();
        outer_entries.sort();
        assert_eq!(
            outer_entries,
            ["canary"],
            "nested Cargo wrote outside target"
        );
        assert_eq!(
            fs::read(outer.join("canary")).expect("outer canary"),
            b"outer compilation remains untouched"
        );
        assert!(target.join("debug/deps").is_dir());
        assert!(target.join("debug/.fingerprint").is_dir());
        assert!(target
            .join("debug")
            .join(format!("zoen-relay{}", std::env::consts::EXE_SUFFIX))
            .is_file());
    }
}
