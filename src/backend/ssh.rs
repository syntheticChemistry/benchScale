// SPDX-License-Identifier: AGPL-3.0-or-later
//! SSH client utilities for LibvirtBackend
//!
//! Provides SSH connection management and command execution for VM access.

use crate::{Error, Result};
use russh::client::AuthResult;
use russh::{ChannelMsg, Disconnect, client};
use std::sync::Arc;
use tracing::{debug, info};

/// SSH client for VM access
pub struct SshClient {
    session: client::Handle<ClientHandler>,
    ip: String,
}

struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        // Accept all server keys (for testing)
        // In production, verify against known_hosts
        Ok(true)
    }
}

impl SshClient {
    /// Connect to a VM via SSH using password authentication
    pub async fn connect(ip: &str, port: u16, user: &str, password: &str) -> Result<Self> {
        info!("Connecting to SSH: {}@{}:{}", user, ip, port);

        let config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(30)),
            ..<client::Config as Default>::default()
        };

        let mut session = client::connect(Arc::new(config), (ip, port), ClientHandler)
            .await
            .map_err(|e| Error::Backend(format!("SSH connection failed: {}", e)))?;

        // Authenticate with password
        let auth_res = session
            .authenticate_password(user, password)
            .await
            .map_err(|e| Error::Backend(format!("SSH authentication failed: {}", e)))?;

        // Check authentication result
        match auth_res {
            AuthResult::Success => {
                info!("SSH authentication successful");
            }
            _ => {
                return Err(Error::Backend(format!(
                    "SSH authentication failed: {:?}",
                    auth_res
                )));
            }
        }

        info!("SSH connection established to {}", ip);

        Ok(Self {
            session,
            ip: ip.to_string(),
        })
    }

    /// Connect to a VM via SSH using key-based authentication.
    ///
    /// If `key_path` is `None`, tries the default `~/.ssh/id_rsa` and
    /// `~/.ssh/id_ed25519` paths in order.
    pub async fn connect_with_key(
        ip: &str,
        port: u16,
        user: &str,
        key_path: Option<&std::path::Path>,
    ) -> Result<Self> {
        info!("Connecting to SSH (key auth): {}@{}:{}", user, ip, port);

        let config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(30)),
            ..<client::Config as Default>::default()
        };

        let mut session = client::connect(Arc::new(config), (ip, port), ClientHandler)
            .await
            .map_err(|e| Error::Backend(format!("SSH connection failed: {}", e)))?;

        let key = Self::load_private_key(key_path)?;
        let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(
            Arc::new(key),
            None,
        );

        let auth_res = session
            .authenticate_publickey(user, key_with_alg)
            .await
            .map_err(|e| Error::Backend(format!("SSH key authentication failed: {}", e)))?;

        match auth_res {
            AuthResult::Success => {
                info!("SSH key authentication successful");
            }
            _ => {
                return Err(Error::Backend(format!(
                    "SSH key authentication failed: {:?}",
                    auth_res
                )));
            }
        }

        Ok(Self {
            session,
            ip: ip.to_string(),
        })
    }

    /// Load a private key from file, trying common locations if path is None.
    fn load_private_key(
        key_path: Option<&std::path::Path>,
    ) -> Result<russh::keys::PrivateKey> {
        if let Some(path) = key_path {
            let key = russh::keys::load_secret_key(path, None)
                .map_err(|e| Error::Backend(format!(
                    "Failed to load SSH key {}: {}", path.display(), e
                )))?;
            return Ok(key);
        }

        let home = dirs::home_dir()
            .ok_or_else(|| Error::Backend("Cannot determine home directory".to_string()))?;

        for name in ["id_ed25519", "id_rsa"] {
            let candidate = home.join(".ssh").join(name);
            if candidate.exists() {
                match russh::keys::load_secret_key(&candidate, None) {
                    Ok(key) => {
                        debug!("Loaded SSH key from {}", candidate.display());
                        return Ok(key);
                    }
                    Err(e) => {
                        debug!("Skipping {}: {}", candidate.display(), e);
                    }
                }
            }
        }

        Err(Error::Backend(
            "No usable SSH private key found (tried ~/.ssh/id_ed25519, ~/.ssh/id_rsa)".to_string()
        ))
    }

    /// Execute a command and return the combined stdout (convenience wrapper).
    pub async fn exec_stdout(&mut self, command: &str) -> Result<String> {
        let (exit_code, stdout, stderr) = self.execute(&[command.to_string()]).await?;
        if exit_code != 0 {
            return Err(Error::Backend(format!(
                "Command failed (exit {}): {}", exit_code,
                if stderr.is_empty() { &stdout } else { &stderr }
            )));
        }
        Ok(stdout)
    }

    /// Push file data to the remote host by writing through a shell channel.
    ///
    /// Uses base64 encoding to safely transfer binary data over the SSH
    /// channel. For files under ~1 MiB this is simpler and more portable
    /// than SFTP.
    pub async fn push_data(
        &mut self,
        data: &[u8],
        remote_path: &str,
    ) -> Result<()> {
        let b64 = base64::encode(data);
        let cmd = format!("echo '{}' | base64 -d > {}", b64, remote_path);
        let (exit_code, _, stderr) = self.execute(&[cmd]).await?;
        if exit_code != 0 {
            return Err(Error::Backend(format!(
                "push_data to {} failed: {}", remote_path, stderr
            )));
        }
        Ok(())
    }

    /// Fetch file data from the remote host.
    pub async fn fetch_data(&mut self, remote_path: &str) -> Result<Vec<u8>> {
        let cmd = format!("base64 {}", remote_path);
        let (exit_code, stdout, stderr) = self.execute(&[cmd]).await?;
        if exit_code != 0 {
            return Err(Error::Backend(format!(
                "fetch_data from {} failed: {}", remote_path, stderr
            )));
        }
        let decoded = data_encoding::BASE64
            .decode(stdout.trim().as_bytes())
            .map_err(|e| Error::Backend(format!("base64 decode failed: {}", e)))?;
        Ok(decoded)
    }

    /// Execute a command on the remote VM
    pub async fn execute(&mut self, command: &[String]) -> Result<(i32, String, String)> {
        let cmd = command.join(" ");
        debug!("Executing SSH command: {}", cmd);

        let mut channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| Error::Backend(format!("Failed to open SSH channel: {}", e)))?;

        channel
            .exec(true, cmd)
            .await
            .map_err(|e| Error::Backend(format!("Failed to execute command: {}", e)))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code: u32 = 0;

        loop {
            let msg = channel.wait().await;

            match msg {
                Some(ChannelMsg::Data { ref data }) => {
                    stdout.extend_from_slice(data);
                }
                Some(ChannelMsg::ExtendedData { ref data, ext }) => {
                    if ext == 1 {
                        // stderr
                        stderr.extend_from_slice(data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = exit_status;
                }
                Some(ChannelMsg::Eof) | None => {
                    break;
                }
                _ => {}
            }
        }

        channel
            .eof()
            .await
            .map_err(|e| Error::Backend(format!("Failed to send EOF: {}", e)))?;

        channel
            .close()
            .await
            .map_err(|e| Error::Backend(format!("Failed to close channel: {}", e)))?;

        let stdout_str = String::from_utf8_lossy(&stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&stderr).to_string();

        debug!("Command exit code: {}", exit_code);
        if !stdout_str.is_empty() {
            debug!("stdout: {}", stdout_str);
        }
        if !stderr_str.is_empty() {
            debug!("stderr: {}", stderr_str);
        }

        Ok((exit_code as i32, stdout_str, stderr_str))
    }

    /// Copy a file to the remote VM using SFTP
    pub async fn copy_file(&mut self, local_path: &str, remote_path: &str) -> Result<()> {
        info!("Copying {} to {}:{}", local_path, self.ip, remote_path);

        // Open SFTP channel
        let channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| Error::Backend(format!("Failed to open SFTP channel: {}", e)))?;

        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| Error::Backend(format!("Failed to request SFTP subsystem: {}", e)))?;

        // Read local file
        let data = tokio::fs::read(local_path)
            .await
            .map_err(|e| Error::Backend(format!("Failed to read local file: {}", e)))?;

        // For now, fall back to scp-like approach via shell
        // Full SFTP implementation would require russh-sftp crate
        drop(channel);

        // Use scp via ssh command as workaround
        let temp_path = format!(
            "/tmp/{}",
            std::path::Path::new(local_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
        );

        // Create file with content via shell
        let base64_data = base64::encode(&data);
        let create_cmd = format!("echo '{}' | base64 -d > {}", base64_data, temp_path);

        self.execute(&[create_cmd]).await?;

        // Move to final location (might need sudo)
        let move_cmd = format!("mv {} {}", temp_path, remote_path);
        self.execute(&[move_cmd]).await?;

        info!("File copied successfully");
        Ok(())
    }

    /// Disconnect SSH session
    pub async fn disconnect(self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await
            .map_err(|e| Error::Backend(format!("Failed to disconnect: {}", e)))?;
        Ok(())
    }
}

// Helper to encode data as base64
mod base64 {
    use data_encoding::BASE64;

    pub fn encode(data: &[u8]) -> String {
        BASE64.encode(data)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_encode_matches_data_encoding() {
        assert_eq!(super::base64::encode(b"hello"), "aGVsbG8=");
    }
}
