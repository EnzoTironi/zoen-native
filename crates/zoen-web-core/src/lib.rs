//! Browser boundary. Device signing keys belong to nonexportable WebCrypto, never Rust.

mod client;
mod link;
pub mod media;
mod primary;
mod wire;

pub use client::{Client, DocumentResponse, Update};
pub use link::{complete_link, create_link, LinkedAccount, PendingLink};
pub use primary::Primary;
pub use wire::{decode_server, hello, request, ServerMessage};

#[cfg(target_arch = "wasm32")]
mod browser;
