// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright © 2024-2025 DataScienceBioLab

//! QEMU Guest Agent (QGA) client over virtio-serial.
//!
//! Communicates with `qemu-guest-agent` running inside a VM through its
//! Unix socket at `/var/lib/libvirt/qemu/channel/target/<vm>.org.qemu.guest_agent.0`.
//!
//! This avoids the need for SSH (no network, no keys, no auth) and works
//! the instant the guest kernel loads the virtio module.
//!
//! Supported commands:
//! - `guest-ping` — liveness check
//! - `guest-info` — agent version and supported commands
//! - `guest-exec` — run a command on the guest
//! - `guest-exec-status` — poll for command completion

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{debug, warn};

use crate::{Error, Result};

static SYNC_COUNTER: AtomicU64 = AtomicU64::new(1);

/// QGA JSON-RPC request envelope.
#[derive(Debug, Serialize)]
struct QgaRequest<'a> {
    execute: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<serde_json::Value>,
}

/// A lightweight QEMU Guest Agent client.
///
/// Connects to the guest agent via its virtio-serial Unix socket and
/// issues JSON-RPC commands.
pub struct QgaClient {
    socket_path: PathBuf,
    timeout: Duration,
}

/// Result of `guest-exec`.
#[derive(Debug, Clone)]
pub struct GuestExecResult {
    /// Guest PID of the executed command.
    pub pid: i64,
    /// Exit code once the process has exited.
    pub exit_code: Option<i64>,
    /// Captured stdout (base64-decoded).
    pub stdout: Option<String>,
    /// Captured stderr (base64-decoded).
    pub stderr: Option<String>,
    /// Whether the process has exited.
    pub exited: bool,
}

/// Information returned by `guest-info`.
#[derive(Debug, Clone, Deserialize)]
pub struct GuestInfo {
    /// Agent version string.
    pub version: String,
    /// Commands the agent supports.
    #[serde(default)]
    pub supported_commands: Vec<SupportedCommand>,
}

/// A single command supported by the guest agent.
#[derive(Debug, Clone, Deserialize)]
pub struct SupportedCommand {
    /// Command name (e.g. `guest-ping`).
    pub name: String,
    /// Whether the command is currently enabled.
    pub enabled: bool,
}

impl QgaClient {
    /// Create a new QGA client for a given VM name.
    ///
    /// The socket path is derived from the VM name following libvirt's
    /// default channel naming convention.
    pub fn for_vm(vm_name: &str) -> Self {
        let socket_path = PathBuf::from(format!(
            "/var/lib/libvirt/qemu/channel/target/{vm_name}.org.qemu.guest_agent.0"
        ));
        Self {
            socket_path,
            timeout: Duration::from_secs(5),
        }
    }

    /// Create a QGA client from an explicit socket path.
    pub fn from_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
            timeout: Duration::from_secs(5),
        }
    }

    /// Set the per-command timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Return the socket path for diagnostics.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Issue `guest-sync` followed by a command and read the response.
    ///
    /// QGA requires a `guest-sync` handshake before each command to
    /// flush any stale data from previous sessions.
    async fn call(
        &self,
        command: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let stream = tokio::time::timeout(
            self.timeout,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| Error::Monitoring("QGA connect timed out".into()))?
        .map_err(|e| Error::Monitoring(format!("QGA socket connect failed: {e}")))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // 1. guest-sync handshake — ensures we're aligned with the agent
        let sync_id = SYNC_COUNTER.fetch_add(1, Ordering::Relaxed);
        let sync_req = serde_json::json!({
            "execute": "guest-sync",
            "arguments": { "id": sync_id }
        });
        let mut sync_bytes = serde_json::to_vec(&sync_req)
            .map_err(|e| Error::Monitoring(format!("JSON encode: {e}")))?;
        sync_bytes.push(b'\n');
        writer.write_all(&sync_bytes).await.map_err(|e| {
            Error::Monitoring(format!("QGA write guest-sync: {e}"))
        })?;

        // Read sync response
        let mut line = String::new();
        tokio::time::timeout(self.timeout, reader.read_line(&mut line))
            .await
            .map_err(|_| Error::Monitoring("QGA guest-sync timed out".into()))?
            .map_err(|e| Error::Monitoring(format!("QGA read guest-sync: {e}")))?;
        debug!("QGA guest-sync response: {}", line.trim());

        // 2. Issue the actual command
        let req = QgaRequest {
            execute: command,
            arguments,
        };
        let mut req_bytes = serde_json::to_vec(&req)
            .map_err(|e| Error::Monitoring(format!("JSON encode: {e}")))?;
        req_bytes.push(b'\n');
        writer.write_all(&req_bytes).await.map_err(|e| {
            Error::Monitoring(format!("QGA write {command}: {e}"))
        })?;

        // 3. Read response
        let mut resp_line = String::new();
        tokio::time::timeout(self.timeout, reader.read_line(&mut resp_line))
            .await
            .map_err(|_| Error::Monitoring(format!("QGA {command} timed out")))?
            .map_err(|e| Error::Monitoring(format!("QGA read {command}: {e}")))?;

        let resp: serde_json::Value = serde_json::from_str(resp_line.trim())
            .map_err(|e| Error::Monitoring(format!("QGA parse {command}: {e}")))?;

        if let Some(err) = resp.get("error") {
            return Err(Error::Monitoring(format!(
                "QGA {command} error: {}",
                err
            )));
        }

        Ok(resp.get("return").cloned().unwrap_or(serde_json::Value::Null))
    }

    /// Ping the guest agent — returns `true` if the agent responds.
    pub async fn ping(&self) -> bool {
        match self.call("guest-ping", None).await {
            Ok(_) => true,
            Err(e) => {
                debug!("QGA ping failed: {e}");
                false
            }
        }
    }

    /// Query guest agent info (version, supported commands).
    pub async fn info(&self) -> Result<GuestInfo> {
        let val = self.call("guest-info", None).await?;

        let version = val
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let supported_commands: Vec<SupportedCommand> = val
            .get("supported_commands")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        Ok(GuestInfo {
            version,
            supported_commands,
        })
    }

    /// Execute a command on the guest.
    ///
    /// Returns the PID immediately. Use `exec_wait` to block until
    /// the command finishes.
    pub async fn exec(&self, command: &str, args: &[&str]) -> Result<i64> {
        let mut arg_list = Vec::with_capacity(args.len());
        for a in args {
            arg_list.push(serde_json::Value::String((*a).to_string()));
        }

        let arguments = serde_json::json!({
            "path": command,
            "arg": arg_list,
            "capture-output": true,
        });

        let val = self.call("guest-exec", Some(arguments)).await?;
        val.get("pid")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| Error::Monitoring("QGA guest-exec: no pid in response".into()))
    }

    /// Poll `guest-exec-status` until the process exits or timeout.
    pub async fn exec_status(&self, pid: i64) -> Result<GuestExecResult> {
        let arguments = serde_json::json!({ "pid": pid });
        let val = self.call("guest-exec-status", Some(arguments)).await?;

        let exited = val.get("exited").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let exit_code = val.get("exitcode").and_then(serde_json::Value::as_i64);

        let stdout = val
            .get("out-data")
            .and_then(serde_json::Value::as_str)
            .and_then(decode_base64);
        let stderr = val
            .get("err-data")
            .and_then(serde_json::Value::as_str)
            .and_then(decode_base64);

        Ok(GuestExecResult {
            pid,
            exit_code,
            stdout,
            stderr,
            exited,
        })
    }

    /// Execute a command and wait for it to finish.
    ///
    /// Polls `guest-exec-status` in a loop with backoff until the
    /// command exits or the timeout is reached.
    pub async fn exec_wait(
        &self,
        command: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GuestExecResult> {
        let pid = self.exec(command, args).await?;
        let deadline = tokio::time::Instant::now() + timeout;
        let mut poll_interval = Duration::from_millis(100);

        loop {
            tokio::time::sleep(poll_interval).await;
            let status = self.exec_status(pid).await?;
            if status.exited {
                return Ok(status);
            }
            if tokio::time::Instant::now() > deadline {
                warn!("QGA exec_wait timed out for pid {pid}");
                return Err(Error::Monitoring(format!(
                    "QGA command (pid {pid}) did not exit within {timeout:?}"
                )));
            }
            poll_interval = (poll_interval * 2).min(Duration::from_secs(2));
        }
    }
}

fn decode_base64(b64: &str) -> Option<String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_from_vm_name() {
        let client = QgaClient::for_vm("lithoSpore-validation");
        assert_eq!(
            client.socket_path().to_str().unwrap(),
            "/var/lib/libvirt/qemu/channel/target/lithoSpore-validation.org.qemu.guest_agent.0"
        );
    }

    #[test]
    fn custom_socket_path() {
        let client = QgaClient::from_socket("/tmp/test.sock");
        assert_eq!(client.socket_path().to_str().unwrap(), "/tmp/test.sock");
    }

    #[test]
    fn timeout_builder() {
        let client = QgaClient::for_vm("test")
            .with_timeout(Duration::from_secs(30));
        assert_eq!(client.timeout, Duration::from_secs(30));
    }
}
