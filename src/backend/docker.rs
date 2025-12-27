//! Docker backend implementation using bollard

use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, UploadToContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions, InspectNetworkOptions};
use bollard::service::Ipam;
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::default::Default;
use tracing::{debug, info, warn};

use super::{Backend, ExecResult, NetworkInfo, NodeInfo, NodeStatus};
use crate::{Error, Result};

/// Docker backend using bollard
pub struct DockerBackend {
    docker: Docker,
    use_hardened: bool,
}

impl DockerBackend {
    /// Create a new Docker backend
    pub fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults().map_err(Error::Docker)?;

        Ok(Self {
            docker,
            use_hardened: false,
        })
    }

    /// Create a new Docker backend with hardened images
    pub fn new_hardened() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults().map_err(Error::Docker)?;

        Ok(Self {
            docker,
            use_hardened: true,
        })
    }

    /// Get the appropriate image name (hardened or standard)
    fn get_image(&self, base_image: &str) -> String {
        if self.use_hardened {
            // Use Docker hardened images when available
            match base_image {
                "ubuntu" => "docker.io/dockerhardened/ubuntu:latest".to_string(),
                "alpine" => "docker.io/dockerhardened/alpine:latest".to_string(),
                "debian" => "docker.io/dockerhardened/debian:latest".to_string(),
                _ => base_image.to_string(),
            }
        } else {
            base_image.to_string()
        }
    }

    /// Ensure image is pulled
    async fn ensure_image(&self, image: &str) -> Result<()> {
        info!("Pulling image: {}", image);

        let mut stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        debug!("Image pull: {}", status);
                    }
                }
                Err(e) => return Err(Error::Docker(e)),
            }
        }

        Ok(())
    }

    /// Apply network conditions using tc (traffic control)
    async fn apply_tc_rules(
        &self,
        container_id: &str,
        latency_ms: Option<u32>,
        packet_loss_percent: Option<f32>,
        bandwidth_kbps: Option<u32>,
    ) -> Result<()> {
        // Build tc command for network simulation
        if latency_ms.is_some() || packet_loss_percent.is_some() || bandwidth_kbps.is_some() {
            let mut tc_cmd = vec![
                "tc".to_string(),
                "qdisc".to_string(),
                "add".to_string(),
                "dev".to_string(),
                "eth0".to_string(),
                "root".to_string(),
                "netem".to_string(),
            ];

            if let Some(latency) = latency_ms {
                tc_cmd.push("delay".to_string());
                tc_cmd.push(format!("{}ms", latency));
            }

            if let Some(loss) = packet_loss_percent {
                tc_cmd.push("loss".to_string());
                tc_cmd.push(format!("{}%", loss));
            }

            if let Some(bandwidth) = bandwidth_kbps {
                tc_cmd.push("rate".to_string());
                tc_cmd.push(format!("{}kbit", bandwidth));
            }

            // Execute tc command
            let result = self.exec_command(container_id, tc_cmd).await?;
            if !result.success() {
                warn!("TC command failed: {} - {}", result.stdout, result.stderr);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Backend for DockerBackend {
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo> {
        info!("Creating network: {} (subnet: {})", name, subnet);

        let ipam_config = bollard::service::IpamConfig {
            subnet: Some(subnet.to_string()),
            ..Default::default()
        };

        let ipam = Ipam {
            driver: Some("default".to_string()),
            config: Some(vec![ipam_config]),
            options: None,
        };

        let config = CreateNetworkOptions {
            name,
            check_duplicate: true,
            driver: "bridge",
            internal: false,
            ipam,
            ..Default::default()
        };

        let response = self.docker.create_network(config).await?;

        // Get network details
        let network = self
            .docker
            .inspect_network(name, None::<InspectNetworkOptions<String>>)
            .await?;

        let (subnet_str, gateway) = if let Some(ipam) = network.ipam {
            let subnet = ipam
                .config
                .as_ref()
                .and_then(|configs| configs.iter().next())
                .and_then(|config| config.subnet.clone())
                .unwrap_or_else(|| subnet.to_string());

            let gw = ipam
                .config
                .and_then(|configs| configs.into_iter().next())
                .and_then(|config| config.gateway)
                .unwrap_or_else(|| "unknown".to_string());

            (subnet, gw)
        } else {
            (subnet.to_string(), "unknown".to_string())
        };

        Ok(NetworkInfo {
            name: name.to_string(),
            id: response.id.unwrap_or_default(),
            subnet: subnet_str,
            gateway,
        })
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        info!("Deleting network: {}", name);
        self.docker.remove_network(name).await?;
        Ok(())
    }

    async fn create_node(
        &self,
        name: &str,
        image: &str,
        network: &str,
        env: HashMap<String, String>,
    ) -> Result<NodeInfo> {
        info!(
            "Creating node: {} (image: {}, network: {})",
            name, image, network
        );

        // Get appropriate image (hardened or standard)
        let image = self.get_image(image);

        // Ensure image is available
        self.ensure_image(&image).await?;

        // Convert env to Vec<String>
        let env_vec: Vec<String> = env.iter().map(|(k, v)| format!("{}={}", k, v)).collect();

        let mut endpoint_config: HashMap<String, bollard::models::EndpointSettings> = HashMap::new();
        endpoint_config.insert(
            network.to_string(),
            Default::default(),
        );

        let config = Config {
            image: Some(image.clone()),
            env: Some(env_vec),
            cmd: Some(vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "sleep infinity".to_string(),
            ]),
            hostname: Some(name.to_string()),
            host_config: Some(bollard::models::HostConfig {
                cap_add: Some(vec!["NET_ADMIN".to_string()]), // For tc (traffic control)
                ..Default::default()
            }),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name,
            platform: None,
        };
        let response = self.docker.create_container(Some(options), config).await?;

        // Connect to network after creation
        self.docker
            .connect_network(
                network,
                ConnectNetworkOptions {
                    container: response.id.as_str(),
                    endpoint_config: Default::default(),
                },
            )
            .await?;

        // Start the container
        self.start_node(&response.id).await?;

        // Get container info
        self.get_node(&response.id).await
    }

    async fn start_node(&self, node_id: &str) -> Result<()> {
        info!("Starting node: {}", node_id);
        self.docker
            .start_container(node_id, None::<StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    async fn stop_node(&self, node_id: &str) -> Result<()> {
        info!("Stopping node: {}", node_id);
        self.docker
            .stop_container(node_id, None::<StopContainerOptions>)
            .await?;
        Ok(())
    }

    async fn delete_node(&self, node_id: &str) -> Result<()> {
        info!("Deleting node: {}", node_id);
        self.docker
            .remove_container(
                node_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        Ok(())
    }

    async fn get_node(&self, node_id: &str) -> Result<NodeInfo> {
        let container = self.docker.inspect_container(node_id, None).await?;

        let name = container
            .name
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();
        let container_id = container.id.unwrap_or_default();

        let status = if let Some(state) = container.state {
            if state.running.unwrap_or(false) {
                NodeStatus::Running
            } else {
                NodeStatus::Stopped
            }
        } else {
            NodeStatus::Stopped
        };

        let (network, ip_address) = container
            .network_settings
            .and_then(|ns| ns.networks)
            .and_then(|networks| {
                networks.iter().next().map(|(net_name, endpoint)| {
                    (
                        net_name.clone(),
                        endpoint.ip_address.clone().unwrap_or_default(),
                    )
                })
            })
            .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

        Ok(NodeInfo {
            id: container_id.clone(),
            name,
            container_id,
            ip_address,
            network,
            status,
            metadata: HashMap::new(),
        })
    }

    async fn list_nodes(&self, network: &str) -> Result<Vec<NodeInfo>> {
        let containers = self.docker.list_containers::<String>(None).await?;

        let mut nodes = vec![];
        for container in containers {
            if let Some(network_settings) = container.network_settings {
                if let Some(networks) = network_settings.networks {
                    if networks.contains_key(network) {
                        if let Some(id) = container.id {
                            if let Ok(node) = self.get_node(&id).await {
                                nodes.push(node);
                            }
                        }
                    }
                }
            }
        }

        Ok(nodes)
    }

    async fn exec_command(&self, node_id: &str, command: Vec<String>) -> Result<ExecResult> {
        debug!("Executing command in {}: {:?}", node_id, command);

        let exec = self
            .docker
            .create_exec(
                node_id,
                CreateExecOptions {
                    cmd: Some(command),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let mut stdout_output = String::new();
        let mut stderr_output = String::new();

        if let StartExecResults::Attached { output, .. } =
            self.docker.start_exec(&exec.id, None).await?
        {
            let mut output_stream = output;
            while let Some(result) = output_stream.next().await {
                match result? {
                    LogOutput::StdOut { message } => {
                        stdout_output.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        stderr_output.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        let inspect = self.docker.inspect_exec(&exec.id).await?;
        let exit_code = inspect.exit_code.unwrap_or(1);

        Ok(ExecResult {
            exit_code,
            stdout: stdout_output,
            stderr: stderr_output,
        })
    }

    async fn copy_to_node(&self, node_id: &str, src_path: &str, dest_path: &str) -> Result<()> {
        info!("Copying {} to {}:{}", src_path, node_id, dest_path);

        // Read file content
        let content = tokio::fs::read(src_path).await?;

        // Create tar archive
        let mut ar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        let filename = std::path::Path::new(src_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        ar.append_data(&mut header, filename, content.as_slice())
            .map_err(|e| Error::Other(format!("Failed to create tar: {}", e)))?;

        let tar_data = ar
            .into_inner()
            .map_err(|e| Error::Other(format!("Failed to finalize tar: {}", e)))?;

        // Upload to container
        self.docker
            .upload_to_container(
                node_id,
                Some(UploadToContainerOptions {
                    path: dest_path,
                    ..Default::default()
                }),
                tar_data.into(),
            )
            .await?;

        Ok(())
    }

    async fn get_logs(&self, node_id: &str) -> Result<String> {
        let mut output = String::new();

        let mut stream = self.docker.logs(
            node_id,
            Some(LogsOptions {
                stdout: true,
                stderr: true,
                tail: "all",
                ..Default::default()
            }),
        );

        while let Some(result) = stream.next().await {
            match result? {
                LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                    output.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }

        Ok(output)
    }

    async fn apply_network_conditions(
        &self,
        node_id: &str,
        latency_ms: Option<u32>,
        packet_loss_percent: Option<f32>,
        bandwidth_kbps: Option<u32>,
    ) -> Result<()> {
        info!("Applying network conditions to node: {}", node_id);
        self.apply_tc_rules(node_id, latency_ms, packet_loss_percent, bandwidth_kbps)
            .await
    }

    async fn is_available(&self) -> Result<bool> {
        match self.docker.ping().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

impl Default for DockerBackend {
    fn default() -> Self {
        Self::new().expect("Failed to create Docker backend")
    }
}
