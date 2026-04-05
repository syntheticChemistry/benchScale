// SPDX-License-Identifier: AGPL-3.0-or-later
//! Test scenario definitions and execution

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::Result;

/// Test scenario definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestScenario {
    /// Scenario name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Test steps
    pub steps: Vec<TestStep>,
    /// Timeout for entire scenario
    pub timeout: Option<Duration>,
}

/// Individual test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    /// Step name
    pub name: String,
    /// Target node
    pub node: String,
    /// Command to execute
    pub command: Vec<String>,
    /// Expected exit code (default: 0)
    #[serde(default)]
    pub expected_exit_code: i64,
    /// Timeout for this step
    pub timeout: Option<Duration>,
}

/// Result of executing a test scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Scenario name
    pub scenario: String,
    /// Success status
    pub success: bool,
    /// Step results
    pub steps: Vec<StepResult>,
    /// Total duration
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
}

/// Result of executing a test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name
    pub name: String,
    /// Success status
    pub success: bool,
    /// Exit code
    pub exit_code: i64,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Step duration
    pub duration: Duration,
}

/// Test runner for executing scenarios
pub struct TestRunner {
    scenarios: Vec<TestScenario>,
}

impl TestRunner {
    /// Create a new test runner
    pub fn new() -> Self {
        Self {
            scenarios: Vec::new(),
        }
    }

    /// Add a scenario to the runner
    pub fn add_scenario(&mut self, scenario: TestScenario) {
        self.scenarios.push(scenario);
    }

    /// Load scenarios from YAML file
    pub async fn load_scenarios_from_file(&mut self, path: &str) -> Result<()> {
        let content = tokio::fs::read_to_string(path).await?;
        let scenarios: Vec<TestScenario> = serde_yaml::from_str(&content)?;
        self.scenarios.extend(scenarios);
        Ok(())
    }

    /// Execute all scenarios
    pub async fn run_all(
        &self,
        backend: Arc<dyn crate::backend::Backend>,
        lab_nodes: &HashMap<String, String>, // node_name -> container_id
    ) -> Vec<TestResult> {
        let mut results = Vec::new();

        for scenario in &self.scenarios {
            let result = self
                .run_scenario(backend.clone(), lab_nodes, scenario)
                .await;
            results.push(result);
        }

        results
    }

    /// Execute a single scenario
    pub async fn run_scenario(
        &self,
        backend: Arc<dyn crate::backend::Backend>,
        lab_nodes: &HashMap<String, String>,
        scenario: &TestScenario,
    ) -> TestResult {
        let start = std::time::Instant::now();
        let mut step_results = Vec::new();
        let mut success = true;
        let mut error = None;

        for step in &scenario.steps {
            let step_start = std::time::Instant::now();

            // Find node container ID
            let Some(container_id) = lab_nodes.get(&step.node) else {
                success = false;
                error = Some(format!("Node not found: {}", step.node));
                break;
            };

            // Execute command
            match backend
                .exec_command(container_id, step.command.clone())
                .await
            {
                Ok(exec_result) => {
                    let step_success = exec_result.exit_code == step.expected_exit_code;
                    if !step_success {
                        success = false;
                        error = Some(format!(
                            "Step '{}' failed: expected exit code {}, got {}",
                            step.name, step.expected_exit_code, exec_result.exit_code
                        ));
                    }

                    step_results.push(StepResult {
                        name: step.name.clone(),
                        success: step_success,
                        exit_code: exec_result.exit_code,
                        stdout: exec_result.stdout,
                        stderr: exec_result.stderr,
                        duration: step_start.elapsed(),
                    });

                    if !step_success {
                        break;
                    }
                }
                Err(e) => {
                    success = false;
                    error = Some(format!("Failed to execute step '{}': {}", step.name, e));
                    step_results.push(StepResult {
                        name: step.name.clone(),
                        success: false,
                        exit_code: -1,
                        stdout: String::new(),
                        stderr: format!("Execution error: {}", e),
                        duration: step_start.elapsed(),
                    });
                    break;
                }
            }
        }

        TestResult {
            scenario: scenario.name.clone(),
            success,
            steps: step_results,
            duration: start.elapsed(),
            error,
        }
    }

    /// Get summary of test results
    pub fn summarize_results(results: &[TestResult]) -> TestSummary {
        let total = results.len();
        let passed = results.iter().filter(|r| r.success).count();
        let failed = total - passed;

        TestSummary {
            total,
            passed,
            failed,
            duration: results.iter().map(|r| r.duration).sum(),
        }
    }
}

/// Summary of test execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    /// Total scenarios
    pub total: usize,
    /// Passed scenarios
    pub passed: usize,
    /// Failed scenarios
    pub failed: usize,
    /// Total duration
    pub duration: Duration,
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ExecResult, NetworkInfo, NodeInfo, NodeStatus};
    use async_trait::async_trait;
    use std::collections::HashMap;

    struct MockExecBackend {
        exit: i64,
    }

    #[async_trait]
    impl crate::backend::Backend for MockExecBackend {
        async fn create_network(&self, _name: &str, subnet: &str) -> crate::Result<NetworkInfo> {
            Ok(NetworkInfo {
                name: "n".into(),
                id: "i".into(),
                subnet: subnet.to_string(),
                gateway: "10.0.0.1".into(),
            })
        }
        async fn delete_network(&self, _name: &str) -> crate::Result<()> {
            Ok(())
        }
        async fn create_node(
            &self,
            name: &str,
            _image: &str,
            _network: &str,
            _env: HashMap<String, String>,
        ) -> crate::Result<NodeInfo> {
            Ok(NodeInfo {
                id: name.into(),
                name: name.into(),
                container_id: name.into(),
                ip_address: "127.0.0.1".into(),
                network: "n".into(),
                status: NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }
        async fn start_node(&self, _node_id: &str) -> crate::Result<()> {
            Ok(())
        }
        async fn stop_node(&self, _node_id: &str) -> crate::Result<()> {
            Ok(())
        }
        async fn delete_node(&self, _node_id: &str) -> crate::Result<()> {
            Ok(())
        }
        async fn get_node(&self, node_id: &str) -> crate::Result<NodeInfo> {
            Ok(NodeInfo {
                id: node_id.into(),
                name: node_id.into(),
                container_id: node_id.into(),
                ip_address: "127.0.0.1".into(),
                network: "n".into(),
                status: NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }
        async fn list_nodes(&self, _network: &str) -> crate::Result<Vec<NodeInfo>> {
            Ok(vec![])
        }
        async fn exec_command(
            &self,
            _node_id: &str,
            _command: Vec<String>,
        ) -> crate::Result<ExecResult> {
            Ok(ExecResult {
                exit_code: self.exit,
                stdout: "ok".into(),
                stderr: String::new(),
            })
        }
        async fn copy_to_node(&self, _node_id: &str, _src: &str, _dest: &str) -> crate::Result<()> {
            Ok(())
        }
        async fn get_logs(&self, _node_id: &str) -> crate::Result<String> {
            Ok(String::new())
        }
        async fn apply_network_conditions(
            &self,
            _node_id: &str,
            _latency_ms: Option<u32>,
            _packet_loss_percent: Option<f32>,
            _bandwidth_kbps: Option<u32>,
        ) -> crate::Result<()> {
            Ok(())
        }
        async fn is_available(&self) -> crate::Result<bool> {
            Ok(true)
        }
    }

    #[test]
    fn test_test_scenario_creation() {
        let scenario = TestScenario {
            name: "ping-test".to_string(),
            description: Some("Test network connectivity".to_string()),
            steps: vec![TestStep {
                name: "ping-node-2".to_string(),
                node: "node-1".to_string(),
                command: vec![
                    "ping".to_string(),
                    "-c".to_string(),
                    "3".to_string(),
                    "node-2".to_string(),
                ],
                expected_exit_code: 0,
                timeout: Some(Duration::from_secs(10)),
            }],
            timeout: Some(Duration::from_secs(30)),
        };

        assert_eq!(scenario.name, "ping-test");
        assert_eq!(scenario.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_run_scenario_success_and_summary() {
        let runner = TestRunner::new();
        let scenario = TestScenario {
            name: "s1".into(),
            description: None,
            steps: vec![TestStep {
                name: "step1".into(),
                node: "node-a".into(),
                command: vec!["true".into()],
                expected_exit_code: 0,
                timeout: None,
            }],
            timeout: None,
        };
        let mut nodes = HashMap::new();
        nodes.insert("node-a".into(), "cid-a".into());
        let backend = Arc::new(MockExecBackend { exit: 0 }) as Arc<dyn crate::backend::Backend>;
        let res = runner.run_scenario(backend, &nodes, &scenario).await;
        assert!(res.success);
        assert!(res.error.is_none());

        let summary = TestRunner::summarize_results(&[res]);
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 0);
    }

    #[tokio::test]
    async fn test_run_scenario_node_missing() {
        let runner = TestRunner::new();
        let scenario = TestScenario {
            name: "s2".into(),
            description: None,
            steps: vec![TestStep {
                name: "x".into(),
                node: "missing".into(),
                command: vec![],
                expected_exit_code: 0,
                timeout: None,
            }],
            timeout: None,
        };
        let backend = Arc::new(MockExecBackend { exit: 0 }) as Arc<dyn crate::backend::Backend>;
        let res = runner
            .run_scenario(backend, &HashMap::new(), &scenario)
            .await;
        assert!(!res.success);
        assert!(res.error.unwrap().contains("Node not found"));
    }

    #[tokio::test]
    async fn test_run_scenario_wrong_exit_code() {
        let runner = TestRunner::new();
        let scenario = TestScenario {
            name: "s3".into(),
            description: None,
            steps: vec![TestStep {
                name: "bad".into(),
                node: "n".into(),
                command: vec!["false".into()],
                expected_exit_code: 0,
                timeout: None,
            }],
            timeout: None,
        };
        let mut nodes = HashMap::new();
        nodes.insert("n".into(), "c".into());
        let backend = Arc::new(MockExecBackend { exit: 7 }) as Arc<dyn crate::backend::Backend>;
        let res = runner.run_scenario(backend, &nodes, &scenario).await;
        assert!(!res.success);
        assert!(res.error.unwrap().contains("exit code"));
    }
}
