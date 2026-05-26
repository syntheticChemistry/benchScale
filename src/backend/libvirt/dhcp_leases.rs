// SPDX-License-Identifier: AGPL-3.0-or-later
//! Safe RAII wrapper for `virNetworkGetDHCPLeases` output.
//!
//! All `unsafe` is confined to this module. Callers only ever interact with
//! safe Rust types ([`Ipv4Lease`], iterators). The `libc` import is required
//! because `libc::free` must release the C-heap array allocated by libvirt;
//! neither `rustix` nor `nix` provides a substitute for that.

use libc;
use std::ffi::CStr;
use std::ptr;
use virt::network::Network;
use virt::sys;

/// Owns the lease pointer array returned by `virNetworkGetDHCPLeases` and frees it on drop.
///
/// This is an opaque, non-`Send`/`Sync` handle — callers consume it via
/// [`ipv4_leases()`](Self::ipv4_leases) which copies data into safe Rust structs.
pub(crate) struct LeaseList {
    leases: *mut sys::virNetworkDHCPLeasePtr,
    count: i32,
}

impl LeaseList {
    /// Query DHCP leases for `network`.
    ///
    /// `mac_filter` restricts results to a single MAC address. Pass `None` to
    /// retrieve all leases.
    ///
    /// # Errors
    /// Returns the negative libvirt error code when `virNetworkGetDHCPLeases` fails.
    pub(crate) fn fetch(
        network: &Network,
        mac_filter: Option<&CStr>,
        flags: u32,
    ) -> Result<Self, i32> {
        let mac_ptr = mac_filter.map_or(ptr::null(), CStr::as_ptr);
        let mut leases: *mut sys::virNetworkDHCPLeasePtr = ptr::null_mut();
        // SAFETY: `network.as_ptr()` is a valid `virNetwork*` for the duration of this call.
        // `mac_ptr` is either null (all leases) or a valid NUL-terminated C string from CStr.
        // `leases` is a valid out-parameter; libvirt writes the array pointer and returns the count.
        let n = unsafe {
            sys::virNetworkGetDHCPLeases(network.as_ptr(), mac_ptr, ptr::addr_of_mut!(leases), flags)
        };
        if n < 0 {
            return Err(n);
        }
        Ok(Self { leases, count: n })
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.count <= 0 || self.leases.is_null()
    }

    fn len(&self) -> usize {
        if self.count <= 0 {
            0
        } else {
            self.count as usize
        }
    }

    /// Parse all IPv4 leases into safe high-level structs.
    ///
    /// This is the only way to access lease data — callers never touch raw pointers.
    pub(crate) fn ipv4_leases(&self, network_name: &str) -> Vec<Ipv4Lease> {
        let n = self.len();
        if n == 0 || self.leases.is_null() {
            return Vec::new();
        }

        // SAFETY: `self.leases` points to `self.count` valid pointers allocated by libvirt.
        // Each non-null pointer is a valid `virNetworkDHCPLease` owned by `self` until `Drop`.
        let lease_slice = unsafe { std::slice::from_raw_parts(self.leases, n) };

        let mut out = Vec::with_capacity(n);
        for &lease_ptr in lease_slice {
            if lease_ptr.is_null() {
                continue;
            }
            // SAFETY: non-null pointer confirmed above; libvirt guarantees struct validity
            // until `virNetworkDHCPLeaseFree` is called (which happens in our Drop).
            let raw = unsafe { &*lease_ptr };
            if raw.type_ != sys::VIR_IP_ADDR_TYPE_IPV4 as i32 {
                continue;
            }
            // SAFETY: `ipaddr`, `mac`, `hostname` are NUL-terminated C strings from libvirt,
            // or null (handled by `c_str_or_empty`).
            let ip_raw = unsafe { c_str_or_empty(raw.ipaddr) };
            let ip_address = ip_raw
                .split('/')
                .next()
                .unwrap_or(&ip_raw)
                .to_string();
            out.push(Ipv4Lease {
                mac_address: unsafe { c_str_or_empty(raw.mac) },
                ip_address,
                hostname: unsafe { c_str_or_empty(raw.hostname) },
                network: network_name.to_string(),
            });
        }
        out
    }
}

/// A parsed IPv4 DHCP lease (fully safe, no raw pointers).
#[derive(Debug, Clone)]
pub(crate) struct Ipv4Lease {
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: String,
    pub network: String,
}

/// Convert a potentially-null C string pointer to an owned `String`.
///
/// # Safety
/// `p` must be either null or a valid NUL-terminated C string that remains
/// valid for the duration of this call.
unsafe fn c_str_or_empty(p: *mut libc::c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: caller guarantees `p` is a valid NUL-terminated C string.
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

impl Drop for LeaseList {
    fn drop(&mut self) {
        if self.leases.is_null() || self.count <= 0 {
            return;
        }
        let n = self.len();
        // SAFETY: `leases` points to `count` pointers allocated by libvirt; each non-null entry
        // must be freed with `virNetworkDHCPLeaseFree`, then the array is freed with `libc::free`.
        unsafe {
            let slice = std::slice::from_raw_parts(self.leases, n);
            for &lease in slice {
                if !lease.is_null() {
                    sys::virNetworkDHCPLeaseFree(lease);
                }
            }
            libc::free(self.leases.cast::<libc::c_void>());
        }
        self.leases = ptr::null_mut();
        self.count = 0;
    }
}
