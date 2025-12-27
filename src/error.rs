//! Error types for benchScale

use thiserror::Error;

/// Result type for benchScale operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur in benchScale
#[derive(Error, Debug)]
pub enum Error {
    /// Docker-related errors
    #[error("Docker error: {0}")]
    Docker(#[from] bollard::errors::Error),

    /// Backend operation errors
    #[error("Backend error: {0}")]
    Backend(String),

    /// Topology parsing errors
    #[error("Topology error: {0}")]
    Topology(String),

    /// Network simulation errors
    #[error("Network error: {0}")]
    Network(String),

    /// Lab operation errors
    #[error("Lab error: {0}")]
    Lab(String),

    /// Test execution errors
    #[error("Test error: {0}")]
    Test(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing errors
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Generic errors
    #[error("{0}")]
    Other(String),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Other(s.to_string())
    }
}

