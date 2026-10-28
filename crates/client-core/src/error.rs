//! Shared application failures for native and JSON clients.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{}{message}", crate::i18n::ts("Armazenamento: ", "Storage: "))]
    Storage { message: String },
    #[error("{}{what}", crate::i18n::ts("Não encontrado: ", "Not found: "))]
    NotFound { what: String },
    #[error("{reason}")]
    Forbidden { reason: String },
    #[error("{reason}")]
    Invalid { reason: String },
    #[error("{reason}")]
    Stale { reason: String },
    /// Only a current, unaccepted private-draft intent refused before append.
    #[error("{reason}")]
    DraftRejected { reason: String },
}
