// SPDX-License-Identifier: AGPL-3.0-or-later
//! IPC compliance testing for primal health endpoints.
//!
//! Validates that primals respond correctly to mandatory JSON-RPC 2.0
//! methods: `health.liveness`, `health.readiness`, `health.check`.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Convert `Duration::as_millis()` (u128) to u64, saturating at `u64::MAX`.
fn millis_u64(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Result of a single compliance check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceResult {
    /// Method that was tested.
    pub method: String,
    /// Whether the method responded with a valid JSON-RPC 2.0 result.
    pub passed: bool,
    /// Response time in milliseconds.
    pub response_ms: u64,
    /// Error message if the check failed.
    pub error: Option<String>,
}

/// Full compliance report for a primal endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Endpoint that was tested.
    pub endpoint: String,
    /// Individual method results.
    pub results: Vec<ComplianceResult>,
    /// Whether all mandatory methods passed.
    pub compliant: bool,
}

/// Validates IPC compliance of primal endpoints via newline-delimited
/// JSON-RPC 2.0 over TCP.
pub struct IpcComplianceValidator {
    connect_timeout: Duration,
    request_timeout: Duration,
}

impl IpcComplianceValidator {
    /// Create a new validator with default timeouts (5s connect, 10s request).
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
        }
    }

    /// Override the connection timeout.
    pub fn with_connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = d;
        self
    }

    /// Override the per-request timeout.
    pub fn with_request_timeout(mut self, d: Duration) -> Self {
        self.request_timeout = d;
        self
    }

    /// Validate a primal at the given TCP address.
    ///
    /// Tests the three mandatory health methods and returns a full report.
    pub async fn validate(&self, addr: SocketAddr) -> ComplianceReport {
        let mandatory_methods = ["health.liveness", "health.readiness", "health.check"];
        let mut results = Vec::new();

        for method in &mandatory_methods {
            let result = self.test_method(addr, method).await;
            results.push(result);
        }

        let compliant = results.iter().all(|r| r.passed);

        ComplianceReport {
            endpoint: addr.to_string(),
            results,
            compliant,
        }
    }

    /// Test a single JSON-RPC method against a TCP endpoint.
    pub async fn test_method(&self, addr: SocketAddr, method: &str) -> ComplianceResult {
        let start = Instant::now();

        let stream = match timeout(self.connect_timeout, TcpStream::connect(addr)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return ComplianceResult {
                    method: method.to_string(),
                    passed: false,
                    response_ms: millis_u64(start.elapsed()),
                    error: Some(format!("connect failed: {e}")),
                };
            }
            Err(_) => {
                return ComplianceResult {
                    method: method.to_string(),
                    passed: false,
                    response_ms: millis_u64(start.elapsed()),
                    error: Some("connect timeout".into()),
                };
            }
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": {},
            "id": 1
        });

        let mut payload = serde_json::to_vec(&request).unwrap_or_default();
        payload.push(b'\n');

        let (reader, mut writer) = stream.into_split();

        if let Err(e) = writer.write_all(&payload).await {
            return ComplianceResult {
                method: method.to_string(),
                passed: false,
                response_ms: millis_u64(start.elapsed()),
                error: Some(format!("write failed: {e}")),
            };
        }

        let mut lines = BufReader::new(reader).lines();

        match timeout(self.request_timeout, lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                let elapsed = millis_u64(start.elapsed());
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(val) => {
                        let is_valid = val.get("jsonrpc").and_then(|v| v.as_str()) == Some("2.0")
                            && val.get("result").is_some()
                            && val.get("error").is_none();

                        ComplianceResult {
                            method: method.to_string(),
                            passed: is_valid,
                            response_ms: elapsed,
                            error: if is_valid {
                                None
                            } else {
                                Some("response missing valid result field".into())
                            },
                        }
                    }
                    Err(e) => ComplianceResult {
                        method: method.to_string(),
                        passed: false,
                        response_ms: elapsed,
                        error: Some(format!("invalid JSON: {e}")),
                    },
                }
            }
            Ok(Ok(None)) => ComplianceResult {
                method: method.to_string(),
                passed: false,
                response_ms: millis_u64(start.elapsed()),
                error: Some("connection closed without response".into()),
            },
            Ok(Err(e)) => ComplianceResult {
                method: method.to_string(),
                passed: false,
                response_ms: millis_u64(start.elapsed()),
                error: Some(format!("read error: {e}")),
            },
            Err(_) => ComplianceResult {
                method: method.to_string(),
                passed: false,
                response_ms: millis_u64(start.elapsed()),
                error: Some("response timeout".into()),
            },
        }
    }
}

impl Default for IpcComplianceValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compliance_result_serialization() {
        let result = ComplianceResult {
            method: "health.liveness".into(),
            passed: true,
            response_ms: 42,
            error: None,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("health.liveness"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_compliance_report_compliant() {
        let report = ComplianceReport {
            endpoint: "127.0.0.1:9000".into(),
            results: vec![
                ComplianceResult {
                    method: "health.liveness".into(),
                    passed: true,
                    response_ms: 1,
                    error: None,
                },
                ComplianceResult {
                    method: "health.readiness".into(),
                    passed: true,
                    response_ms: 1,
                    error: None,
                },
                ComplianceResult {
                    method: "health.check".into(),
                    passed: true,
                    response_ms: 1,
                    error: None,
                },
            ],
            compliant: true,
        };
        assert!(report.compliant);
    }

    #[test]
    fn test_compliance_report_non_compliant() {
        let report = ComplianceReport {
            endpoint: "127.0.0.1:9000".into(),
            results: vec![
                ComplianceResult {
                    method: "health.liveness".into(),
                    passed: true,
                    response_ms: 1,
                    error: None,
                },
                ComplianceResult {
                    method: "health.readiness".into(),
                    passed: false,
                    response_ms: 5000,
                    error: Some("timeout".into()),
                },
            ],
            compliant: false,
        };
        assert!(!report.compliant);
    }

    #[test]
    fn test_validator_builder() {
        let v = IpcComplianceValidator::new()
            .with_connect_timeout(Duration::from_secs(1))
            .with_request_timeout(Duration::from_secs(2));
        assert_eq!(v.connect_timeout, Duration::from_secs(1));
        assert_eq!(v.request_timeout, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_validate_unreachable_endpoint() {
        let v = IpcComplianceValidator::new().with_connect_timeout(Duration::from_millis(100));
        let report = v.validate("127.0.0.1:1".parse().expect("addr")).await;
        assert!(!report.compliant);
        assert_eq!(report.results.len(), 3);
        for r in &report.results {
            assert!(!r.passed);
        }
    }
}
