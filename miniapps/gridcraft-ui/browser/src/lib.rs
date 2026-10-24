//! Managed shell only: no upstream preferences, browser file pickers, URI opening,
//! collaboration, storage or postMessage transport. The official Apps SDK owns RPC.
#[cfg(target_arch = "wasm32")]
mod api;
pub mod layout;
#[cfg(target_arch = "wasm32")]
mod shell;

#[cfg(target_arch = "wasm32")]
pub use api::{on_change, on_export, request, retire, start};
