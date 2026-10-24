//! Managed PDF editor: the official Apps SDK owns RPC and Core owns persistence.
#[cfg(target_arch = "wasm32")]
mod api;
pub mod layout;
#[cfg(target_arch = "wasm32")]
mod shell;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
#[cfg(any(target_arch = "wasm32", test))]
mod viewport;
#[cfg(target_arch = "wasm32")]
pub use api::{on_change, on_export, request, retire, start};
