//! Non-authoritative classes for optional operational diagnostics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolObservation {
    Held,
    Blocked,
    UnknownTool,
    PreparationRefused,
    AdmissionError,
    HostOk,
    HostError,
}
