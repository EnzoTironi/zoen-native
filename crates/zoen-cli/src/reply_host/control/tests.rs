//! Real socket ownership and stale process transport, no fake authority.
use super::*;
use std::os::unix::fs::symlink;
type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Home(PathBuf);
impl Home {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("reply-control-test")?);
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        if let Ok(path) = socket_path(&self.0, false) {
            let _ = fs::remove_dir(path.parent().unwrap_or(&path));
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[tokio::test]
async fn live_duplicate_and_foreign_files_are_not_reclaimed() -> TestResult {
    let home = Home::new()?;
    let server = Server::bind(&home.0)?;
    let path = socket_path(&home.0, false)?;
    assert_eq!(socket_meta(&path)?.permissions().mode() & 0o777, 0o600);
    assert!(Server::bind(&home.0).is_err());
    drop(server);
    assert!(!path.exists());
    fs::write(&path, b"preserve foreign contents")?;
    assert!(Server::bind(&home.0).is_err());
    assert_eq!(fs::read(&path)?, b"preserve foreign contents");
    fs::remove_file(&path)?;
    symlink(home.0.join("unrelated"), &path)?;
    assert!(Server::bind(&home.0).is_err());
    assert!(fs::symlink_metadata(&path)?.file_type().is_symlink());
    fs::remove_file(&path)?;
    Ok(())
}
#[tokio::test]
async fn dead_socket_recovers_and_drop_preserves_replacement() -> TestResult {
    let home = Home::new()?;
    let path = socket_path(&home.0, true)?;
    let abandoned = std::os::unix::net::UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    drop(abandoned);
    let server = Server::bind(&home.0)?;
    fs::remove_file(&path)?;
    fs::write(&path, b"replacement is not our socket")?;
    drop(server);
    assert_eq!(fs::read(&path)?, b"replacement is not our socket");
    fs::remove_file(&path)?;
    Ok(())
}
#[tokio::test]
async fn private_long_home_uses_short_transport_and_public_home_is_rejected() -> TestResult {
    let home = Home::new()?;
    let long = home.0.join("private-host-directory".repeat(8));
    fs::DirBuilder::new().mode(0o700).create(&long)?;
    let server = Server::bind(&long)?;
    let path = socket_path(&long, false)?;
    assert!(path.as_os_str().as_bytes().len() < 104);
    drop(server);
    fs::remove_dir(path.parent().ok_or("socket directory")?)?;
    fs::set_permissions(&home.0, fs::Permissions::from_mode(0o755))?;
    assert!(Server::bind(&home.0).is_err());
    fs::set_permissions(&home.0, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[tokio::test]
async fn raw_peer_is_bounded_and_cannot_add_authority_fields() -> TestResult {
    for bytes in [
        vec![b'x'; protocol::MAX_INPUT + 1],
        br#"{"command":"advance","run":"x","approved":true}"#.to_vec(),
        b"not a command".to_vec(),
    ] {
        let (mut input, mut output) = Stream::pair()?;
        let writer = async {
            let _ = output.write_all(&bytes).await;
            let _ = output.shutdown().await;
        };
        let (result, ()) = tokio::join!(read_command(&mut input), writer);
        assert!(result.is_err());
    }
    let (mut input, _idle_peer) = Stream::pair()?;
    assert!(read_command(&mut input).await.is_err());
    Ok(())
}
