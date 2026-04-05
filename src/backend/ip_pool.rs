// SPDX-License-Identifier: AGPL-3.0-or-later
//! IP Pool Management
//!
//! Provides deterministic IP address allocation for VMs, eliminating DHCP race conditions.
//! This module ensures that each VM gets a unique IP address without relying on timing
//! or external DHCP state.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{Error, Result};
use ipnetwork::IpNetwork;

/// IP address pool manager
///
/// Manages a pool of available IP addresses for VM assignment.
/// Thread-safe and async-compatible.
///
/// # Example
/// ```
/// use benchscale::backend::ip_pool::IpPool;
/// use std::net::Ipv4Addr;
///
/// # async fn example() -> anyhow::Result<()> {
/// let pool = IpPool::new(
///     "192.168.122.0/24".to_string(),
///     Ipv4Addr::new(192, 168, 122, 10),
///     Ipv4Addr::new(192, 168, 122, 250),
/// )?;
///
/// let ip1 = pool.allocate().await?;
/// let ip2 = pool.allocate().await?;
/// assert_ne!(ip1, ip2);
///
/// pool.release(ip1).await;
/// # Ok(())
/// # }
/// ```
pub struct IpPool {
    inner: Arc<Mutex<IpPoolInner>>,
    // Cached immutable values for sync access
    network: String,
    range_start: Ipv4Addr,
    range_end: Ipv4Addr,
}

struct IpPoolInner {
    allocated: HashSet<Ipv4Addr>,
    next_candidate: Ipv4Addr,
}

impl IpPool {
    /// Create a new IP pool from string IP range
    ///
    /// Convenience method that parses IP addresses from strings.
    ///
    /// # Arguments
    /// * `start_ip` - First allocatable IP as string (e.g., "192.168.122.10")
    /// * `end_ip` - Last allocatable IP as string (e.g., "192.168.122.250")
    ///
    /// # Example
    /// ```
    /// use benchscale::backend::ip_pool::IpPool;
    ///
    /// let pool = IpPool::from_range("192.168.122.10", "192.168.122.250")?;
    /// # Ok::<(), benchscale::Error>(())
    /// ```
    pub fn from_range(start_ip: &str, end_ip: &str) -> Result<Self> {
        let range_start: Ipv4Addr = start_ip
            .parse()
            .map_err(|e| Error::Backend(format!("Invalid start IP '{}': {}", start_ip, e)))?;

        let range_end: Ipv4Addr = end_ip
            .parse()
            .map_err(|e| Error::Backend(format!("Invalid end IP '{}': {}", end_ip, e)))?;

        // Extract network prefix from start IP (assume /24)
        let octets = range_start.octets();
        let network_cidr = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);

        Self::new(network_cidr, range_start, range_end)
    }

    /// Create a new IP pool
    ///
    /// # Arguments
    /// * `network` - Network CIDR (e.g., "192.168.122.0/24")
    /// * `range_start` - First allocatable IP (e.g., 192.168.122.10)
    /// * `range_end` - Last allocatable IP (e.g., 192.168.122.250)
    ///
    /// # Example
    /// ```
    /// use benchscale::backend::ip_pool::IpPool;
    /// use std::net::Ipv4Addr;
    ///
    /// let pool = IpPool::new(
    ///     "192.168.122.0/24".to_string(),
    ///     Ipv4Addr::new(192, 168, 122, 10),
    ///     Ipv4Addr::new(192, 168, 122, 250),
    /// )?;
    /// # Ok::<(), benchscale::Error>(())
    /// ```
    pub fn new(network_cidr: String, range_start: Ipv4Addr, range_end: Ipv4Addr) -> Result<Self> {
        // Validate range ordering
        if range_start > range_end {
            return Err(Error::Backend(format!(
                "Invalid IP range: start ({}) > end ({})",
                range_start, range_end
            )));
        }

        // Validate CIDR format by parsing it
        let network = network_cidr
            .parse::<IpNetwork>()
            .map_err(|e| Error::Backend(format!("Invalid CIDR '{}': {}", network_cidr, e)))?;

        // Validate that both range_start and range_end are within the network
        if !network.contains(range_start.into()) {
            return Err(Error::Backend(format!(
                "Range start {} is outside network {}",
                range_start, network_cidr
            )));
        }
        if !network.contains(range_end.into()) {
            return Err(Error::Backend(format!(
                "Range end {} is outside network {}",
                range_end, network_cidr
            )));
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(IpPoolInner {
                allocated: HashSet::new(),
                next_candidate: range_start,
            })),
            network: network_cidr, // Store as String
            range_start,
            range_end,
        })
    }

    /// Create a default IP pool for libvirt's default network
    ///
    /// Uses the standard libvirt default network (192.168.122.0/24)
    /// with allocation range 192.168.122.10-250.
    ///
    /// # Example
    /// ```
    /// use benchscale::backend::ip_pool::IpPool;
    ///
    /// let pool = IpPool::default_libvirt();
    /// ```
    pub fn default_libvirt() -> Self {
        Self::new(
            "192.168.122.0/24".to_string(),
            Ipv4Addr::new(192, 168, 122, 10),
            Ipv4Addr::new(192, 168, 122, 250),
        )
        .expect("Default libvirt pool should always be valid")
    }

    /// Allocate the next available IP address
    ///
    /// Returns an IP address that is guaranteed to be unique among currently
    /// allocated addresses. This operation is atomic and thread-safe.
    ///
    /// # Returns
    /// The allocated IP address
    ///
    /// # Errors
    /// Returns error if the pool is exhausted (all IPs allocated)
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let ip = pool.allocate().await?;
    /// println!("Allocated: {}", ip);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn allocate(&self) -> Result<Ipv4Addr> {
        let mut inner = self.inner.lock().await;

        // Normalize start IP to be within range
        let mut start_ip = inner.next_candidate;
        if start_ip < self.range_start || start_ip > self.range_end {
            start_ip = self.range_start;
        }

        // Calculate total IPs in the pool to prevent infinite loops
        let pool_size = Self::range_size(self.range_start, self.range_end);
        let mut checked = 0;
        let mut current_ip = start_ip;

        while checked < pool_size {
            // Ensure current_ip is in valid range (wrap around)
            if current_ip > self.range_end {
                current_ip = self.range_start;
            }
            if current_ip < self.range_start {
                current_ip = self.range_start;
            }

            // Check if current IP is available
            if !inner.allocated.contains(&current_ip) {
                // Found an available IP!
                inner.allocated.insert(current_ip);

                // Update next_candidate for faster future allocations
                inner.next_candidate = Self::next_ip(current_ip);
                if inner.next_candidate > self.range_end {
                    inner.next_candidate = self.range_start;
                }

                return Ok(current_ip);
            }

            // Move to next IP in range
            current_ip = Self::next_ip(current_ip);
            checked += 1;
        }

        // We've checked all IPs in the pool - exhausted
        Err(Error::Backend(format!(
            "IP pool exhausted: all {} addresses in range {}-{} are allocated",
            inner.allocated.len(),
            self.range_start,
            self.range_end
        )))
    }

    /// Allocate a specific IP address
    ///
    /// Attempts to reserve a specific IP address. Useful for VMs that need
    /// predictable IPs (e.g., DNS servers, gateways).
    ///
    /// # Arguments
    /// * `ip` - The specific IP address to allocate
    ///
    /// # Returns
    /// The allocated IP address (same as input)
    ///
    /// # Errors
    /// Returns error if:
    /// - IP is outside the allocatable range
    /// - IP is already allocated
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # use std::net::Ipv4Addr;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let dns_ip = pool.allocate_specific(Ipv4Addr::new(192, 168, 122, 53)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn allocate_specific(&self, ip: Ipv4Addr) -> Result<Ipv4Addr> {
        let mut inner = self.inner.lock().await;

        // Validate IP is in range
        if ip < self.range_start || ip > self.range_end {
            return Err(Error::Backend(format!(
                "IP {} is outside allocatable range {}-{}",
                ip, self.range_start, self.range_end
            )));
        }

        // Check if already allocated
        if inner.allocated.contains(&ip) {
            return Err(Error::Backend(format!("IP {} is already allocated", ip)));
        }

        // Allocate it
        inner.allocated.insert(ip);
        Ok(ip)
    }

    /// Release an IP address back to the pool
    ///
    /// Makes the IP address available for future allocations.
    /// Safe to call multiple times with the same IP.
    ///
    /// # Arguments
    /// * `ip` - The IP address to release
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let ip = pool.allocate().await?;
    /// pool.release(ip).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn release(&self, ip: Ipv4Addr) {
        let mut inner = self.inner.lock().await;
        inner.allocated.remove(&ip);

        // Optimize: if this IP is before our next candidate, update next_candidate
        // to this IP for faster future allocations
        if ip < inner.next_candidate {
            inner.next_candidate = ip;
        }
    }

    /// Get the number of currently allocated IPs
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let ip = pool.allocate().await?;
    /// assert_eq!(pool.allocated_count().await, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn allocated_count(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.allocated.len()
    }

    /// Get the total capacity of the pool
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// let pool = IpPool::default_libvirt();
    /// assert_eq!(pool.capacity(), 241); // 192.168.122.10-250 inclusive
    /// ```
    pub fn capacity(&self) -> usize {
        Self::range_size(self.range_start, self.range_end)
    }

    /// Get the number of available (unallocated) IPs
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let available_before = pool.available_count().await;
    /// let ip = pool.allocate().await?;
    /// assert_eq!(pool.available_count().await, available_before - 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn available_count(&self) -> usize {
        let inner = self.inner.lock().await;
        Self::range_size(self.range_start, self.range_end) - inner.allocated.len()
    }

    /// Check if an IP is currently allocated
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = IpPool::default_libvirt();
    /// let ip = pool.allocate().await?;
    /// assert!(pool.is_allocated(ip).await);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_allocated(&self, ip: Ipv4Addr) -> bool {
        let inner = self.inner.lock().await;
        inner.allocated.contains(&ip)
    }

    /// Get the network CIDR for this pool
    ///
    /// # Example
    /// ```
    /// # use benchscale::backend::ip_pool::IpPool;
    /// let pool = IpPool::default_libvirt();
    /// assert_eq!(pool.network(), "192.168.122.0/24");
    /// ```
    pub fn network(&self) -> String {
        self.network.clone()
    }

    // Helper: calculate the size of an IP range (inclusive)
    fn range_size(start: Ipv4Addr, end: Ipv4Addr) -> usize {
        let start_u32 = u32::from(start);
        let end_u32 = u32::from(end);
        (end_u32 - start_u32 + 1) as usize
    }

    // Helper: get the next IP address
    fn next_ip(ip: Ipv4Addr) -> Ipv4Addr {
        let ip_u32 = u32::from(ip);
        let next_u32 = ip_u32 + 1;

        // Convert back to Ipv4Addr
        Ipv4Addr::from(next_u32)
    }
}

impl Clone for IpPool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            network: self.network.clone(),
            range_start: self.range_start,
            range_end: self.range_end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allocate_unique_ips() {
        let pool = IpPool::default_libvirt();

        let ip1 = pool.allocate().await.unwrap();
        let ip2 = pool.allocate().await.unwrap();
        let ip3 = pool.allocate().await.unwrap();

        // All IPs should be unique
        assert_ne!(ip1, ip2);
        assert_ne!(ip2, ip3);
        assert_ne!(ip1, ip3);

        // All should be in range
        assert!(ip1 >= Ipv4Addr::new(192, 168, 122, 10));
        assert!(ip1 <= Ipv4Addr::new(192, 168, 122, 250));
    }

    #[tokio::test]
    async fn test_release_and_reallocate() {
        let pool = IpPool::default_libvirt();

        let ip1 = pool.allocate().await.unwrap();
        let ip2 = pool.allocate().await.unwrap();

        pool.release(ip1).await;

        let ip3 = pool.allocate().await.unwrap();
        // After releasing ip1, it should be available again
        // (might be ip1 or another IP depending on allocation strategy)
        assert_ne!(ip3, ip2);
    }

    #[tokio::test]
    async fn test_allocate_specific() {
        let pool = IpPool::default_libvirt();

        let target_ip = Ipv4Addr::new(192, 168, 122, 100);
        let allocated = pool.allocate_specific(target_ip).await.unwrap();

        assert_eq!(allocated, target_ip);
        assert!(pool.is_allocated(target_ip).await);
    }

    #[tokio::test]
    async fn test_allocate_specific_already_allocated() {
        let pool = IpPool::default_libvirt();

        let ip = Ipv4Addr::new(192, 168, 122, 100);
        pool.allocate_specific(ip).await.unwrap();

        // Try to allocate the same IP again
        let result = pool.allocate_specific(ip).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_allocation() {
        let pool = IpPool::default_libvirt();

        // Allocate 10 IPs concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let pool_clone = pool.clone();
                tokio::spawn(async move { pool_clone.allocate().await })
            })
            .collect();

        let mut ips = Vec::new();
        for handle in handles {
            let ip = handle.await.unwrap().unwrap();
            ips.push(ip);
        }

        // All IPs should be unique
        let unique_ips: HashSet<_> = ips.iter().collect();
        assert_eq!(
            unique_ips.len(),
            10,
            "All concurrently allocated IPs should be unique"
        );
    }

    #[tokio::test]
    async fn test_capacity_and_counts() {
        let pool = IpPool::default_libvirt();

        let capacity = pool.capacity();
        assert_eq!(capacity, 241); // 192.168.122.10-250 inclusive

        let ip1 = pool.allocate().await.unwrap();
        assert_eq!(pool.allocated_count().await, 1);
        assert_eq!(pool.available_count().await, capacity - 1);

        let _ip2 = pool.allocate().await.unwrap();
        assert_eq!(pool.allocated_count().await, 2);
        assert_eq!(pool.available_count().await, capacity - 2);

        pool.release(ip1).await;
        assert_eq!(pool.allocated_count().await, 1);
        assert_eq!(pool.available_count().await, capacity - 1);
    }

    #[tokio::test]
    async fn test_pool_exhaustion() {
        // Create a small pool
        let pool = IpPool::new(
            "192.168.122.0/24".to_string(),
            Ipv4Addr::new(192, 168, 122, 10),
            Ipv4Addr::new(192, 168, 122, 12), // Only 3 IPs
        )
        .unwrap();

        // Allocate all IPs
        let _ip1 = pool.allocate().await.unwrap();
        let ip2 = pool.allocate().await.unwrap();
        let _ip3 = pool.allocate().await.unwrap();

        // Try to allocate one more - should fail
        let result = pool.allocate().await;
        assert!(result.is_err());

        // Release one and try again - should work
        pool.release(ip2).await;
        let ip4 = pool.allocate().await;
        assert!(ip4.is_ok());
    }

    // === Error Handling Tests (Phase 1b) ===

    #[test]
    fn test_invalid_cidr_format() {
        let result = IpPool::new(
            "not-a-valid-cidr".to_string(),
            Ipv4Addr::new(192, 168, 122, 10),
            Ipv4Addr::new(192, 168, 122, 50),
        );

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("CIDR") || e.to_string().contains("network"));
        }
    }

    #[test]
    fn test_range_start_after_end() {
        let result = IpPool::new(
            "192.168.122.0/24".to_string(),
            Ipv4Addr::new(192, 168, 122, 100), // Start
            Ipv4Addr::new(192, 168, 122, 50),  // End (before start!)
        );

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("range") || e.to_string().contains("start"));
        }
    }

    #[test]
    fn test_range_outside_network() {
        let result = IpPool::new(
            "192.168.122.0/24".to_string(),
            Ipv4Addr::new(192, 168, 123, 10), // Wrong subnet!
            Ipv4Addr::new(192, 168, 123, 50),
        );

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("network") || e.to_string().contains("range"));
        }
    }

    #[tokio::test]
    async fn test_release_unallocated_ip() {
        let pool = IpPool::default_libvirt();

        let ip = Ipv4Addr::new(192, 168, 122, 100);

        // Release an IP that was never allocated
        // This should succeed (idempotent release) - current implementation doesn't error
        pool.release(ip).await; // Returns () not Result

        // Verify it's not allocated
        assert!(!pool.is_allocated(ip).await);
    }

    #[tokio::test]
    async fn test_double_release() {
        let pool = IpPool::default_libvirt();

        let ip = pool.allocate().await.unwrap();

        // First release
        pool.release(ip).await;

        // Second release of same IP (idempotent)
        pool.release(ip).await;

        // Verify it's not allocated
        assert!(!pool.is_allocated(ip).await);
    }

    #[tokio::test]
    async fn test_allocate_after_release() {
        let pool = IpPool::new(
            "192.168.122.0/24".to_string(),
            Ipv4Addr::new(192, 168, 122, 10),
            Ipv4Addr::new(192, 168, 122, 12), // Only 3 IPs
        )
        .unwrap();

        // Allocate all
        let _ip1 = pool.allocate().await.unwrap();
        let ip2 = pool.allocate().await.unwrap();
        let _ip3 = pool.allocate().await.unwrap();

        // Pool exhausted
        assert!(pool.allocate().await.is_err());

        // Release one
        pool.release(ip2).await;

        // Should be able to allocate again
        let ip4 = pool.allocate().await.unwrap();
        assert_eq!(ip4, ip2, "Should reuse the released IP");
    }
}
