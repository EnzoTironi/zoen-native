//! Managed Word canvas: the official Apps SDK owns RPC and Core owns persistence.
#[cfg(target_arch = "wasm32")]
mod api;
pub mod layout;
#[cfg(target_arch = "wasm32")]
mod shell;
#[cfg(target_arch = "wasm32")]
pub use api::{on_change, on_export, request, retire, start};

#[cfg(any(target_arch = "wasm32", test))]
mod viewport;
