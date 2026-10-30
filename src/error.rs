//! Structured error handling for Chronos.

use std::fmt;
use std::io;

pub type ChronosResult<T> = std::result::Result<T, ChronosError>;

#[derive(Debug)]
pub enum ChronosError {
    Io(io::Error),
    Json(serde_json::Error),
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    Validation(String),
    DataSafety(String),
    SchemaMismatch { found: u32, supported: u32 },
    ActiveSessionGuard(String),
}

impl fmt::Display for ChronosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChronosError::Io(err) => write!(f, "I/O error: {}", err),
            ChronosError::Json(err) => write!(f, "JSON serialization error: {}", err),
            ChronosError::TomlDe(err) => write!(f, "TOML parse error: {}", err),
            ChronosError::TomlSer(err) => write!(f, "TOML format error: {}", err),
            ChronosError::Validation(msg) => write!(f, "Validation error: {}", msg),
            ChronosError::DataSafety(msg) => write!(f, "Data safety guard: {}", msg),
            ChronosError::SchemaMismatch { found, supported } => write!(
                f,
                "Vault schema version {} is newer than supported version {}",
                found, supported
            ),
            ChronosError::ActiveSessionGuard(msg) => write!(f, "Active session guard: {}", msg),
        }
    }
}

impl std::error::Error for ChronosError {}

impl From<io::Error> for ChronosError {
    fn from(err: io::Error) -> Self {
        ChronosError::Io(err)
    }
}

impl From<serde_json::Error> for ChronosError {
    fn from(err: serde_json::Error) -> Self {
        ChronosError::Json(err)
    }
}

impl From<toml::de::Error> for ChronosError {
    fn from(err: toml::de::Error) -> Self {
        ChronosError::TomlDe(err)
    }
}

impl From<toml::ser::Error> for ChronosError {
    fn from(err: toml::ser::Error) -> Self {
        ChronosError::TomlSer(err)
    }
}
