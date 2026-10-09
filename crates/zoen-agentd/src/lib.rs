//! # zoen-agentd
//!
//! The agent runtime's tool side (ADR 0028). An agent's tools run in the cheapest tier that
//! their signed manifest allows:
//!
//! - **T0**: no sandbox, or a WASM component in-process;
//! - **T1**: a microVM for real code (Firecracker in production; gVisor stands in during P0);
//! - **T2**: a browser microVM.
//!
//! A model can never ask for a heavier tier: only a tool's signed [`manifest::ToolManifest`]
//! can, and only a Grant from the owner lets it run there ([`router`]). Sandboxes start lazily
//! on the first call that needs one, stop after an idle period and are torn down at the end of
//! the task ([`session`]). Every second is charged to the owner's daily budget ([`budget`]).

pub mod browser;
pub mod budget;
pub mod manifest;
pub mod router;
pub mod sandbox;
pub mod session;

pub use browser::{BrowserCall, BrowserTools};
pub use manifest::{EgressRule, Needs, SecretBinding, SignedManifest, ToolManifest};
pub use router::{Route, Router, Tier};
pub use sandbox::{ExecOutput, ExecRequest, Lease, SandboxError, SandboxProvider, SandboxSpec};
