// SPDX-License-Identifier: AGPL-3.0-only
//! Topology definitions and YAML parsing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::{Error, Result};

/// Complete topology configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Topology metadata
    pub metadata: TopologyMetadata,
    /// Network configuration
    pub network: NetworkConfig,
    /// Node configurations
    pub nodes: Vec<NodeConfig>,
}

/// Topology metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyMetadata {
    /// Topology name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Version
    pub version: Option<String>,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network name
    pub name: String,
    /// Subnet CIDR
    pub subnet: String,
    /// Simulated network conditions
    #[serde(default)]
    pub conditions: Option<NetworkConditions>,
}

/// Network conditions for simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditions {
    /// Latency in milliseconds
    pub latency_ms: Option<u32>,
    /// Packet loss percentage (0-100)
    pub packet_loss_percent: Option<f32>,
    /// Bandwidth in kbps
    pub bandwidth_kbps: Option<u32>,
}

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node name
    pub name: String,
    /// Docker image to use
    pub image: String,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Port mappings (host:container)
    #[serde(default)]
    pub ports: Vec<String>,
    /// Volumes (host:container)
    #[serde(default)]
    pub volumes: Vec<String>,
    /// Node-specific network conditions (overrides network-level)
    #[serde(default)]
    pub network_conditions: Option<NetworkConditions>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Alternative topology configuration for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyConfig {
    /// Lab name
    pub lab_name: String,
    /// Network subnet
    pub network_subnet: String,
    /// Nodes in the topology
    pub nodes: Vec<TopologyNode>,
}

/// Alternative node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    /// Node name
    pub name: String,
    /// Docker image
    pub image: String,
    /// Simulated latency
    pub latency_ms: Option<u32>,
    /// Simulated packet loss
    pub packet_loss_percent: Option<f32>,
    /// Simulated bandwidth limit
    pub bandwidth_kbps: Option<u32>,
}

impl Topology {
    /// Load topology from a YAML file
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path.as_ref()).await?;
        Self::from_yaml(&content)
    }

    /// Parse topology from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml)
            .map_err(|e| Error::Topology(format!("Failed to parse topology: {}", e)))
    }

    /// Save topology to a YAML file
    pub async fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = self.to_yaml()?;
        tokio::fs::write(path.as_ref(), yaml).await?;
        Ok(())
    }

    /// Convert topology to YAML string
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self)
            .map_err(|e| Error::Topology(format!("Failed to serialize topology: {}", e)))
    }

    /// Validate topology configuration
    pub fn validate(&self) -> Result<()> {
        // Check network subnet is valid CIDR
        if !self.network.subnet.contains('/') {
            return Err(Error::Topology(format!(
                "Invalid subnet CIDR: {}",
                self.network.subnet
            )));
        }

        // Check all nodes have unique names
        let mut names = std::collections::HashSet::new();
        for node in &self.nodes {
            if !names.insert(&node.name) {
                return Err(Error::Topology(format!(
                    "Duplicate node name: {}",
                    node.name
                )));
            }
        }

        // Validate network conditions
        if let Some(conditions) = &self.network.conditions {
            validate_network_conditions(conditions)?;
        }

        // Validate node-specific network conditions
        for node in &self.nodes {
            if let Some(conditions) = &node.network_conditions {
                validate_network_conditions(conditions)?;
            }
        }

        Ok(())
    }

    /// Get node configuration by name
    pub fn get_node(&self, name: &str) -> Option<&NodeConfig> {
        self.nodes.iter().find(|n| n.name == name)
    }

    /// Get effective network conditions for a node
    pub fn get_node_conditions(&self, node_name: &str) -> Option<NetworkConditions> {
        self.get_node(node_name).and_then(|node| {
            // Node-specific conditions override network-level
            node.network_conditions
                .clone()
                .or_else(|| self.network.conditions.clone())
        })
    }
}

impl TopologyConfig {
    /// Convert to new Topology format
    pub fn into_topology(self) -> Topology {
        Topology {
            metadata: TopologyMetadata {
                name: self.lab_name.clone(),
                description: None,
                version: None,
                tags: vec![],
            },
            network: NetworkConfig {
                name: format!("{}-network", self.lab_name),
                subnet: self.network_subnet,
                conditions: None,
            },
            nodes: self
                .nodes
                .iter()
                .map(|node| NodeConfig {
                    name: node.name.clone(),
                    image: node.image.clone(),
                    env: HashMap::new(),
                    ports: vec![],
                    volumes: vec![],
                    network_conditions: Some(NetworkConditions {
                        latency_ms: node.latency_ms,
                        packet_loss_percent: node.packet_loss_percent,
                        bandwidth_kbps: node.bandwidth_kbps,
                    }),
                    metadata: HashMap::new(),
                })
                .collect(),
        }
    }
}

/// Validate network conditions
fn validate_network_conditions(conditions: &NetworkConditions) -> Result<()> {
    if let Some(packet_loss) = conditions.packet_loss_percent
        && !(0.0..=100.0).contains(&packet_loss) {
            return Err(Error::Topology(format!(
                "Packet loss must be between 0 and 100, got: {}",
                packet_loss
            )));
        }

    if let Some(latency) = conditions.latency_ms
        && latency > 10000 {
            return Err(Error::Topology(format!(
                "Latency must be <= 10000ms, got: {}",
                latency
            )));
        }

    Ok(())
}


#[cfg(test)]
mod tests;
