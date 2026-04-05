// SPDX-License-Identifier: AGPL-3.0-or-later
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

    /// Database errors (persistence feature)
    #[cfg(feature = "persistence")]
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Monitoring / senescence errors
    #[error("Monitoring error: {0}")]
    Monitoring(String),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_backend() {
        let err = Error::Backend("test error".to_string());
        assert_eq!(err.to_string(), "Backend error: test error");
    }

    #[test]
    fn test_error_topology() {
        let err = Error::Topology("invalid topology".to_string());
        assert_eq!(err.to_string(), "Topology error: invalid topology");
    }

    #[test]
    fn test_error_network() {
        let err = Error::Network("network failure".to_string());
        assert_eq!(err.to_string(), "Network error: network failure");
    }

    #[test]
    fn test_error_lab() {
        let err = Error::Lab("lab creation failed".to_string());
        assert_eq!(err.to_string(), "Lab error: lab creation failed");
    }

    #[test]
    fn test_error_test() {
        let err = Error::Test("test failed".to_string());
        assert_eq!(err.to_string(), "Test error: test failed");
    }

    #[test]
    fn test_error_other() {
        let err = Error::Other("generic error".to_string());
        assert_eq!(err.to_string(), "generic error");
    }

    #[test]
    fn test_error_from_string() {
        let err: Error = "string error".to_string().into();
        assert_eq!(err.to_string(), "string error");
    }

    #[test]
    fn test_error_from_str() {
        let err: Error = "str error".into();
        assert_eq!(err.to_string(), "str error");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(err.to_string().contains("IO error"));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_error_from_yaml() {
        let yaml_str = "invalid: yaml: data:";
        let yaml_err = serde_yaml::from_str::<serde_yaml::Value>(yaml_str).unwrap_err();
        let err: Error = yaml_err.into();
        assert!(err.to_string().contains("YAML error"));
    }

    #[test]
    fn test_error_from_json() {
        let json_str = "{invalid json}";
        let json_err = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let err: Error = json_err.into();
        assert!(err.to_string().contains("JSON error"));
    }

    #[test]
    fn test_error_debug() {
        let err = Error::Backend("debug test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Backend"));
        assert!(debug_str.contains("debug test"));
    }

    #[test]
    fn test_result_type() {
        let success: Result<i32> = Ok(42);
        assert_eq!(success.unwrap(), 42);

        let failure: Result<i32> = Err(Error::Other("failed".to_string()));
        assert!(failure.is_err());
    }
}
