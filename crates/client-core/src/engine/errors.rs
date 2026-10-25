//! Preserve the existing public Core error contract for fallible local preparation.
use crate::CoreError;

impl From<store::StoreError> for CoreError {
    fn from(e: store::StoreError) -> Self {
        CoreError::Storage {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}

impl From<event_log::AppendError> for CoreError {
    fn from(error: event_log::AppendError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}

impl From<event_log::CryptoError> for CoreError {
    fn from(error: event_log::CryptoError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}

impl From<types::IdError> for CoreError {
    fn from(error: types::IdError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}

impl From<device_link::LinkError> for CoreError {
    fn from(error: device_link::LinkError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}
impl From<event_log::LogError> for CoreError {
    fn from(error: event_log::LogError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}
