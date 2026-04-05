// SPDX-License-Identifier: AGPL-3.0-or-later
//! VM provisioning state helpers: deterministic identity, pool IP bookkeeping, and metadata
//! for desktop VMs using DHCP discovery.

use crate::backend::IpPool;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::Ipv4Addr;

/// Derive a stable QEMU-style MAC address from a VM name (prefix `52:54:00`, hash suffix).
#[must_use]
pub(crate) fn qemu_mac_from_vm_name(name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    format!(
        "52:54:00:{:02x}:{:02x}:{:02x}",
        (hash >> 16) & 0xFF,
        (hash >> 8) & 0xFF,
        hash & 0xFF
    )
}

/// Metadata entries recorded on a desktop VM node after DHCP discovery completes.
#[must_use]
pub(crate) fn desktop_dhcp_node_metadata(mac_address: &str) -> HashMap<String, String> {
    let mut node_meta = HashMap::new();
    node_meta.insert("mac_address".to_string(), mac_address.to_string());
    node_meta.insert("dhcp_mode".to_string(), "true".to_string());
    node_meta
}

/// Release a pool-allocated IPv4 if this provisioning path used the pool.
pub(crate) async fn release_pool_ip_if_needed(
    from_pool: bool,
    allocated_ip: &str,
    pool: &IpPool,
) {
    if !from_pool {
        return;
    }
    if let Ok(ip_addr) = allocated_ip.parse::<Ipv4Addr>() {
        pool.release(ip_addr).await;
    }
}

/// Same as [`release_pool_ip_if_needed`], but schedules release on the runtime (e.g. sync error paths).
pub(crate) fn spawn_release_pool_ip_if_needed(
    from_pool: bool,
    allocated_ip: &str,
    pool: &IpPool,
) {
    if !from_pool {
        return;
    }
    if let Ok(ip_addr) = allocated_ip.parse::<Ipv4Addr>() {
        let pool = pool.clone();
        tokio::spawn(async move {
            pool.release(ip_addr).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::qemu_mac_from_vm_name;

    #[test]
    fn qemu_mac_from_name_is_deterministic() {
        let a = qemu_mac_from_vm_name("bench-vm");
        let b = qemu_mac_from_vm_name("bench-vm");
        assert_eq!(a, b);
        assert!(a.starts_with("52:54:00:"));
    }

    #[test]
    fn qemu_mac_from_distinct_names_differs() {
        let a = qemu_mac_from_vm_name("vm-a");
        let b = qemu_mac_from_vm_name("vm-b");
        assert_ne!(a, b);
    }
}
