use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LogError {
    #[error("event #{seq} could not be serialized for chain verification")]
    Serialization { seq: u64 },
    #[error("evento #{seq} pertence a outro Espaço")]
    WrongSpace { seq: u64 },
    #[error("sequência quebrada: esperava #{expected}, veio #{got}")]
    BadSequence { expected: u64, got: u64 },
    #[error("cadeia quebrada no evento #{seq}")]
    BrokenChain { seq: u64 },
    #[error("hash não confere no evento #{seq} (conteúdo alterado)")]
    BadHash { seq: u64 },
    #[error("assinatura inválida no evento #{seq}")]
    BadSignature { seq: u64 },
    #[error("chave de autor inválida no evento #{seq}")]
    BadAuthorKey { seq: u64 },
    #[error("aparelho não certificado pelo autor no evento #{seq}")]
    BadDeviceCert { seq: u64 },
    #[error("conteúdo assinado ilegível ou diferente do evento #{seq}")]
    BadContent { seq: u64 },
    #[error("o evento #{seq} aponta para uma história que este aparelho não tem")]
    BrokenCausalLink { seq: u64 },
    #[error("o evento #{seq} não diz o que o autor já tinha visto")]
    MissingCausalLink { seq: u64 },
    #[error("o evento #{seq} cria de novo um Espaço que já existe")]
    Recreated { seq: u64 },
}

/// A local append cannot publish a partial identifier or serialized event.
#[derive(Debug, Error)]
pub enum AppendError {
    #[error(transparent)]
    Identifier(#[from] types::IdError),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
