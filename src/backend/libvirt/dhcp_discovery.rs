//! DHCP Lease Discovery for Libvirt VMs
//!
//! Evolution #12: Robust DHCP IP Discovery
//! ========================================
//!
//! This module provides functionality to discover the actual IP address assigned
//! to a VM via DHCP by querying libvirt's DHCP lease database.
//!
//! ## Why This Matters
//!
//! - **Fractal Scaling**: VMs can be created without pre-allocated static IPs
//! - **Cloud-Native**: Works in any libvirt environment with DHCP
//! - **Parallel Creation**: Multiple VMs don't conflict over IP addresses
//! - **Location Agnostic**: VMs self-configure on any network
//!
//! ## Implementation
//!
//! We query `virsh net-dhcp-leases` and match VMs by MAC address, which is the
//! most reliable identifier since hostnames might not be set yet during early boot.

use anyhow::{anyhow, Context, Result};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// A DHCP lease entry from libvirt
#[derive(Debug, Clone)]
pub struct DhcpLease {
    /// MAC address of the VM
    pub mac_address: String,
    /// IP address assigned via DHCP
    pub ip_address: String,
    /// Hostname (may be empty during early boot)
    pub hostname: String,
    /// Network name (usually "default")
    pub network: String,
}

/// Configuration for DHCP discovery
#[derive(Debug, Clone, Copy)]
pub struct DiscoveryConfig {
    /// Maximum time to wait for DHCP lease to appear
    pub max_wait_secs: u64,
    /// Interval between lease checks
    pub retry_interval_secs: u64,
    /// Network name to query (usually "default")
    pub network_name: &'static str,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            max_wait_secs: 60,
            retry_interval_secs: 2,
            network_name: "default",
        }
    }
}

/// Discovers the DHCP-assigned IP address for a VM by its MAC address
///
/// This function polls libvirt's DHCP lease database until the VM's lease appears
/// or the timeout is reached.
///
/// # Arguments
///
/// * `mac_address` - The MAC address of the VM to discover
/// * `config` - Configuration for the discovery process
///
/// # Returns
///
/// The discovered IP address, or an error if the lease couldn't be found
///
/// # Example
///
/// ```no_run
/// use benchscale::backend::libvirt::dhcp_discovery::{discover_dhcp_ip, DiscoveryConfig};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let mac = "52:54:00:12:34:56";
///     let config = DiscoveryConfig::default();
///     let ip = discover_dhcp_ip(mac, config).await?;
///     println!("VM got IP: {}", ip);
///     Ok(())
/// }
/// ```
pub async fn discover_dhcp_ip(mac_address: &str, config: DiscoveryConfig) -> Result<String> {
    info!(
        "🔍 Discovering DHCP IP for MAC {} (max wait: {}s)",
        mac_address, config.max_wait_secs
    );

    let max_attempts = config.max_wait_secs / config.retry_interval_secs;
    let start_time = std::time::Instant::now();

    for attempt in 1..=max_attempts {
        // Query DHCP leases
        match query_dhcp_leases(config.network_name) {
            Ok(leases) => {
                // Look for our MAC address
                if let Some(lease) = leases.iter().find(|l| l.mac_address == mac_address) {
                    info!(
                        "✅ Discovered DHCP IP: {} for MAC {} (after {}s, attempt {})",
                        lease.ip_address,
                        mac_address,
                        start_time.elapsed().as_secs(),
                        attempt
                    );
                    return Ok(lease.ip_address.clone());
                }

                // Log available MACs for debugging
                if attempt == 1 || attempt % 5 == 0 {
                    debug!(
                        "Available DHCP leases: {}",
                        leases
                            .iter()
                            .map(|l| format!("{} -> {}", l.mac_address, l.ip_address))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to query DHCP leases (attempt {}): {}",
                    attempt, e
                );
            }
        }

        if attempt < max_attempts {
            sleep(Duration::from_secs(config.retry_interval_secs)).await;
        }
    }

    Err(anyhow!(
        "Timeout: Could not discover DHCP IP for MAC {} after {}s ({} attempts). \
         VM may not have started or DHCP server may not be responding.",
        mac_address,
        config.max_wait_secs,
        max_attempts
    ))
}

/// Queries libvirt's DHCP lease database for a network
///
/// This function parses the output of `virsh net-dhcp-leases` to extract
/// all current DHCP leases.
///
/// # Arguments
///
/// * `network_name` - The name of the libvirt network (usually "default")
///
/// # Returns
///
/// A vector of all DHCP leases, or an error if the query failed
fn query_dhcp_leases(network_name: &str) -> Result<Vec<DhcpLease>> {
    debug!("Querying DHCP leases for network: {}", network_name);

    let output = Command::new("virsh")
        .args(["net-dhcp-leases", network_name])
        .output()
        .context("Failed to execute virsh net-dhcp-leases")?;

    if !output.status.success() {
        return Err(anyhow!(
            "virsh net-dhcp-leases failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .context("virsh output contains invalid UTF-8")?;

    parse_dhcp_leases(&stdout, network_name)
}

/// Parses the output of `virsh net-dhcp-leases`
///
/// Example output format:
/// ```text
///  Expiry Time           MAC address         Protocol   IP address          Hostname   Client ID or DUID
/// -----------------------------------------------------------------------------------------------------------------------
///  2024-01-03 20:30:42   52:54:00:12:34:56   ipv4       192.168.122.10/24   ubuntu     ff:00:12:34:00:01:00:...
/// ```
fn parse_dhcp_leases(output: &str, network_name: &str) -> Result<Vec<DhcpLease>> {
    let mut leases = Vec::new();

    for line in output.lines().skip(2) {
        // Skip header lines
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            debug!("Skipping malformed lease line: {}", line);
            continue;
        }

        // Extract fields (columns may vary, we need MAC, Protocol, IP)
        // Format: Expiry(2) MAC Protocol IP Hostname(optional) Client-ID(optional)
        let mac_address = parts[2].to_string();
        let protocol = parts[3];
        let ip_with_mask = parts[4];
        let hostname = if parts.len() > 5 {
            parts[5].to_string()
        } else {
            String::new()
        };

        // Only handle IPv4 for now
        if protocol != "ipv4" {
            debug!("Skipping non-IPv4 lease: {}", line);
            continue;
        }

        // Strip CIDR mask (/24) from IP address
        let ip_address = ip_with_mask
            .split('/')
            .next()
            .unwrap_or(ip_with_mask)
            .to_string();

        leases.push(DhcpLease {
            mac_address,
            ip_address,
            hostname,
            network: network_name.to_string(),
        });
    }

    debug!("Parsed {} DHCP leases", leases.len());
    Ok(leases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dhcp_leases() {
        let output = r#"
 Expiry Time           MAC address         Protocol   IP address          Hostname   Client ID or DUID
-----------------------------------------------------------------------------------------------------------------------
 2024-01-03 20:30:42   52:54:00:12:34:56   ipv4       192.168.122.10/24   ubuntu     ff:00:12:34:00:01
 2024-01-03 20:31:15   52:54:00:aa:bb:cc   ipv4       192.168.122.11/24   test-vm    ff:00:aa:bb:cc:01
"#;

        let leases = parse_dhcp_leases(output, "default").unwrap();
        assert_eq!(leases.len(), 2);

        assert_eq!(leases[0].mac_address, "52:54:00:12:34:56");
        assert_eq!(leases[0].ip_address, "192.168.122.10");
        assert_eq!(leases[0].hostname, "ubuntu");

        assert_eq!(leases[1].mac_address, "52:54:00:aa:bb:cc");
        assert_eq!(leases[1].ip_address, "192.168.122.11");
        assert_eq!(leases[1].hostname, "test-vm");
    }

    #[test]
    fn test_parse_empty_output() {
        let output = r#"
 Expiry Time           MAC address         Protocol   IP address          Hostname   Client ID or DUID
-----------------------------------------------------------------------------------------------------------------------
"#;

        let leases = parse_dhcp_leases(output, "default").unwrap();
        assert_eq!(leases.len(), 0);
    }

    #[test]
    fn test_parse_ipv6_filtered() {
        let output = r#"
 Expiry Time           MAC address         Protocol   IP address          Hostname   Client ID or DUID
-----------------------------------------------------------------------------------------------------------------------
 2024-01-03 20:30:42   52:54:00:12:34:56   ipv4       192.168.122.10/24   ubuntu     ff:00:12:34:00:01
 2024-01-03 20:30:42   52:54:00:12:34:56   ipv6       fe80::5054:ff:fe12:3456/128   ubuntu     ff:00:12:34:00:01
"#;

        let leases = parse_dhcp_leases(output, "default").unwrap();
        assert_eq!(leases.len(), 1); // Only IPv4
        assert_eq!(leases[0].ip_address, "192.168.122.10");
    }
}

