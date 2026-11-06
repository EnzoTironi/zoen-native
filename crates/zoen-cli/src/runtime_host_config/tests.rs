use super::*;
use std::{
    fs,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Home(std::path::PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn operator_config_is_private_bounded_and_does_not_echo_bad_content(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = Home(std::env::temp_dir().join(format!(
        "zoen-private-host-config-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )));
    fs::create_dir(&home.0)?;
    let path = home.0.join("config.json");
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&path)?.write_all(b"{\"scope\":\"one\"}")?;
    let value: serde_json::Value = read_private(&path)?;
    assert_eq!(value["scope"], "one");
    fs::write(&path, b"PRIVATE-CREDENTIAL-CANARY invalid JSON")?;
    let error = read_private::<serde_json::Value>(&path)
        .err()
        .ok_or("invalid JSON accepted")?;
    assert!(!error.contains("PRIVATE-CREDENTIAL-CANARY"));
    fs::write(&path, vec![b' '; 32769])?;
    assert!(read_private::<serde_json::Value>(&path).is_err());
    fs::write(&path, b"{}")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let link = home.0.join("link.json");
        symlink(&path, &link)?;
        assert!(read_private::<serde_json::Value>(&link).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
        assert!(read_private::<serde_json::Value>(&path).is_err());
    }
    Ok(())
}
