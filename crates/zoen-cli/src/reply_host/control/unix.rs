//! Private Unix socket. Frames never supply a device, credentials or authority.
use crate::protocol;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream as Stream},
};
use zoen_runtime::ReplyRuntimeHost;
const IO_WAIT: Duration = Duration::from_secs(2);
const RESULT_WAIT: Duration = Duration::from_secs(150);
const SOCKET: &str = "s";

fn private_home(home: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(home).map_err(|_| "control home unavailable")?;
    if !meta.is_dir() || meta.permissions().mode() & 0o777 != 0o700 {
        return Err("live controls require a private host directory (chmod 700)".into());
    }
    Ok(meta)
}
fn socket_path(home: &Path, create: bool) -> Result<PathBuf, String> {
    let meta = private_home(home)?;
    let canonical = fs::canonicalize(home).map_err(|_| "control home unavailable")?;
    let mut digest = Sha256::new();
    digest.update(canonical.as_os_str().as_bytes());
    digest.update(meta.dev().to_le_bytes());
    digest.update(meta.ino().to_le_bytes());
    let digest = hex::encode(digest.finalize());
    // macOS sockaddr_un is too short for ordinary worktree paths. This is only
    // ephemeral transport routing, never the custody home or continuity anchor.
    // Control subprocesses clear their environment; routing must not depend
    // on TMPDIR inherited by only one side of the same local connection.
    let directory = Path::new("/tmp").join(format!("zoen-reply-{}", &digest[..24]));
    if create {
        match fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("private control directory unavailable".into()),
        }
    }
    let actual = private_home(&directory)?;
    if actual.uid() != meta.uid() {
        return Err("foreign control directory".into());
    }
    Ok(directory.join(SOCKET))
}
fn socket_meta(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "control socket unavailable")?;
    if !meta.file_type().is_socket() || meta.permissions().mode() & 0o777 != 0o600 {
        return Err("invalid control socket".into());
    }
    Ok(meta)
}
struct SocketFile {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for SocketFile {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.dev() == self.device && m.ino() == self.inode && m.file_type().is_socket()
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub(crate) struct Server {
    listener: UnixListener,
    _file: SocketFile,
}
impl Server {
    /// Caller already holds HostLock. Stale sockets are not durable app state.
    pub(crate) fn bind(home: &Path) -> Result<Self, String> {
        let path = socket_path(home, true)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                socket_meta(&path)?;
                match UnixStream::connect(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                        fs::remove_file(&path).map_err(|_| "stale control socket unavailable")?;
                    }
                    _ => return Err("control socket already active or unavailable".into()),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("control socket unavailable".into()),
        }
        let listener =
            UnixListener::bind(&path).map_err(|_| "cannot bind private control socket")?;
        let meta = fs::symlink_metadata(&path).map_err(|_| "control socket unavailable")?;
        let file = SocketFile {
            path,
            device: meta.dev(),
            inode: meta.ino(),
        };
        fs::set_permissions(&file.path, fs::Permissions::from_mode(0o600))
            .map_err(|_| "cannot protect control socket")?;
        Ok(Self {
            listener,
            _file: file,
        })
    }
    pub(crate) async fn accept(&self) -> Result<Stream, String> {
        self.listener
            .accept()
            .await
            .map(|(stream, _)| stream)
            .map_err(|_| "control listener unavailable".into())
    }
    /// One bounded request between scheduler cycles; never a concurrent executor.
    pub(crate) async fn respond(host: &ReplyRuntimeHost, mut stream: Stream) -> bool {
        let result = match read_command(&mut stream).await {
            Ok(command) => protocol::execute(host, command).await,
            Err(error) => Err(error),
        };
        let changed = result
            .as_ref()
            .is_ok_and(|v| v["status"] == "policy_installed");
        let response = match result {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error}),
        };
        if let Ok(bytes) = serde_json::to_vec(&response) {
            if bytes.len() <= protocol::MAX_OUTPUT {
                let _ = tokio::time::timeout(IO_WAIT, stream.write_all(&bytes)).await;
            }
        }
        changed
    }
}

async fn read_command(stream: &mut Stream) -> Result<protocol::Command, String> {
    let mut bytes = Vec::new();
    tokio::time::timeout(
        IO_WAIT,
        stream
            .take(protocol::MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes),
    )
    .await
    .map_err(|_| "control input deadline exceeded")?
    .map_err(|_| "control input unavailable")?;
    if bytes.len() > protocol::MAX_INPUT {
        return Err("control input too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid control command".into())
}

pub(crate) fn request(home: &Path, bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > protocol::MAX_INPUT {
        return Err("control input too large".into());
    }
    let _: protocol::Command =
        serde_json::from_slice(bytes).map_err(|_| "invalid control command")?;
    let path = socket_path(home, false)?;
    socket_meta(&path)?;
    let mut stream = UnixStream::connect(path).map_err(|_| "live reply host unavailable")?;
    stream
        .set_write_timeout(Some(IO_WAIT))
        .map_err(|_| "control write unavailable")?;
    stream
        .set_read_timeout(Some(RESULT_WAIT))
        .map_err(|_| "control response unavailable")?;
    stream
        .write_all(bytes)
        .map_err(|_| "control write failed")?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|_| "control write incomplete")?;
    let mut response = Vec::new();
    stream
        .take(protocol::MAX_OUTPUT as u64 + 1)
        .read_to_end(&mut response)
        .map_err(|_| "control response unavailable")?;
    if response.len() > protocol::MAX_OUTPUT {
        return Err("control response too large".into());
    }
    let value: Value = serde_json::from_slice(&response).map_err(|_| "invalid control response")?;
    if value.get("error").is_some() {
        return Err("live reply host rejected control".into());
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "invalid control response".into())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
