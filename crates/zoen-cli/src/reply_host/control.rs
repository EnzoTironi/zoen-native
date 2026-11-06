//! Local operator transport into the existing single custody owner.
#[cfg(unix)]
#[path = "control/unix.rs"]
mod unix;
#[cfg(unix)]
pub(super) use unix::{request, Server};

#[cfg(not(unix))]
mod unavailable {
    use std::path::Path;
    use zoen_runtime::ReplyRuntimeHost;
    pub struct Server;
    pub struct Request;
    impl Server {
        pub fn bind(_: &Path) -> Result<Self, String> {
            Ok(Self)
        }
        pub async fn accept(&self) -> Result<Request, String> {
            std::future::pending().await
        }
        pub async fn respond(_: &ReplyRuntimeHost, _: Request) -> bool {
            false
        }
    }
    pub fn request(_: &Path, _: &[u8]) -> Result<serde_json::Value, String> {
        Err("live host controls require a Unix host".into())
    }
}
#[cfg(not(unix))]
pub(super) use unavailable::{request, Server};
