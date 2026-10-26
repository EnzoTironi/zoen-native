//! The actual CLI must answer help before creating a home or opening SQLite.

use std::{
    error::Error,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, Box<dyn Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "zoen-help-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn run(&self, args: &[String]) -> Result<Output, Box<dyn Error>> {
        Ok(Command::new(env!("CARGO_BIN_EXE_zoen"))
            .current_dir(&self.0)
            .env_remove("ZOEN_HOME")
            .env("ZOEN_BRAIN", "help-must-not-configure-a-provider")
            .args(args)
            .output()?)
    }

    fn pristine(&self) -> Result<(), Box<dyn Error>> {
        assert_eq!(
            std::fs::read_dir(&self.0)?.count(),
            0,
            "help initialized account state in {}",
            self.0.display()
        );
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn argv(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}

#[test]
fn global_and_command_help_never_create_the_default_home() -> Result<(), Box<dyn Error>> {
    for args in [
        argv(&["--help"]),
        argv(&["-h"]),
        argv(&["--help", "-h", "--help"]),
        argv(&["status", "--help"]),
        argv(&["agent-host", "--help"]),
        argv(&["file", "-h"]),
        argv(&["status", "--help", "--help"]),
    ] {
        let scratch = Scratch::new()?;
        let output = scratch.run(&args)?;
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stdout.starts_with(b"usage: zoen "), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        scratch.pristine()?;
    }
    Ok(())
}

#[test]
fn help_never_creates_an_explicit_nonexistent_home() -> Result<(), Box<dyn Error>> {
    for suffix in [argv(&["--help"]), argv(&["status", "-h"])] {
        let scratch = Scratch::new()?;
        let home = scratch.0.join("absent-home");
        assert!(!home.exists());
        let mut args = vec!["--home".into(), home.to_string_lossy().into_owned()];
        args.extend(suffix);
        let output = scratch.run(&args)?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.starts_with(b"usage: zoen "), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert!(!home.exists(), "help created {}", home.display());
        scratch.pristine()?;
    }
    Ok(())
}

#[test]
fn no_arguments_keep_the_original_usage_and_exit_status() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new()?;
    let original = scratch.run(&[])?;
    let explicit = scratch.run(&argv(&["--help"]))?;
    assert_eq!(original.status.code(), Some(2));
    assert!(original.stdout.is_empty());
    assert_eq!(explicit.stdout, original.stderr);
    assert!(explicit.stderr.is_empty());
    assert!(explicit.status.success());
    scratch.pristine()?;
    Ok(())
}
