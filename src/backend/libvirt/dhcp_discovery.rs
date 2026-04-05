// SPDX-License-Identifier: AGPL-3.0-or-later
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
//! We query libvirt network DHCP leases and match VMs by MAC address, which is the
//! most reliable identifier since hostnames might not be set yet during early boot.
//!
//! `libc::c_char` in helpers matches libvirt’s exported C pointer types (virt FFI; not
//! replaceable with rustix/nix).

use super::dhcp_leases::LeaseList;
use anyhow::{Result, anyhow};
use libc;
use std::ffi::CStr;
use std::ptr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use virt::connect::Connect;
use virt::network::Network;

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
                warn!("Failed to query DHCP leases (attempt {}): {}", attempt, e);
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
/// # Arguments
///
/// * `network_name` - The name of the libvirt network (usually "default")
///
/// # Returns
///
/// A vector of all DHCP leases, or an error if the query failed
pub(crate) fn query_dhcp_leases(network_name: &str) -> Result<Vec<DhcpLease>> {
    let mut conn =
        Connect::open(None).map_err(|e| anyhow!("Failed to connect to libvirt: {}", e))?;
    let result = query_dhcp_leases_with_connect(&conn, network_name);
    let _ = conn.close();
    result
}

pub(crate) fn query_dhcp_leases_with_connect(
    conn: &Connect,
    network_name: &str,
) -> Result<Vec<DhcpLease>> {
    debug!("Querying DHCP leases for network: {}", network_name);

    let net = Network::lookup_by_name(conn, network_name)
        .map_err(|e| anyhow!("Failed to lookup network: {}", e))?;

    let list = LeaseList::fetch(&net, ptr::null(), 0).map_err(|_| {
        anyhow!(
            "virNetworkGetDHCPLeases failed: {}",
            virt::error::Error::last_error()
        )
    })?;
    if list.is_empty() {
        return Ok(Vec::new());
    }

    let mut leases = Vec::new();
    for i in 0..list.len() {
        let lease_ptr = list.lease_ptr_at(i);
        if lease_ptr.is_null() {
            continue;
        }
        // SAFETY: `lease_ptr` comes from libvirt's lease array for this query; we only read fields
        // before `list` is dropped (which frees the lease structs).
        let lease = unsafe { &*lease_ptr };
        let mac_address = c_string_from_ptr(lease.mac);
        let ip_raw = c_string_from_ptr(lease.ipaddr);
        let hostname = c_string_from_ptr(lease.hostname);
        let type_ = lease.type_;
        if type_ != virt::sys::VIR_IP_ADDR_TYPE_IPV4 as i32 {
            continue;
        }
        let ip_address = ip_raw
            .split('/')
            .next()
            .unwrap_or(ip_raw.as_str())
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

/// Converts a libvirt NUL-terminated lease field; `libc::c_char` matches libvirt’s C pointers.
fn c_string_from_ptr(p: *mut libc::c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: Libvirt provides NUL-terminated C strings for lease fields; we only read until NUL.
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}
