// SPDX-License-Identifier: AGPL-3.0-or-later
//! Safe RAII wrapper for `virNetworkGetDHCPLeases` output.
//!
//! `libc` remains here for libvirt FFI only: `c_char` / `c_void` match the C ABI, and
//! `libc::free` releases the array allocated by libvirt. Neither `rustix` nor `nix` provides
//! a replacement for deallocating C-heap memory from libvirt.

use libc;
use std::ptr;
use virt::network::Network;
use virt::sys;

/// Owns the lease pointer array returned by `virNetworkGetDHCPLeases` and frees it on drop.
pub(crate) struct LeaseList {
    leases: *mut sys::virNetworkDHCPLeasePtr,
    count: i32,
}

impl LeaseList {
    /// Query DHCP leases for `network`. `mac` is the optional MAC filter (null for all).
    ///
    /// # Errors
    /// Returns the negative libvirt error code when `virNetworkGetDHCPLeases` fails.
    pub(crate) fn fetch(
        network: &Network,
        mac: *const libc::c_char,
        flags: u32,
    ) -> Result<Self, i32> {
        let mut leases: *mut sys::virNetworkDHCPLeasePtr = ptr::null_mut();
        // SAFETY: `network.as_ptr()` is a valid `virNetwork*` for the duration of this call.
        // `leases` is a valid out-parameter; libvirt writes the array pointer and returns the count.
        let n = unsafe {
            sys::virNetworkGetDHCPLeases(network.as_ptr(), mac, ptr::addr_of_mut!(leases), flags)
        };
        if n < 0 {
            return Err(n);
        }
        Ok(Self { leases, count: n })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count <= 0 || self.leases.is_null()
    }

    pub(crate) fn len(&self) -> usize {
        if self.count <= 0 {
            0
        } else {
            self.count as usize
        }
    }

    /// Raw pointer at `index`, or null if out of range.
    pub(crate) fn lease_ptr_at(&self, index: usize) -> sys::virNetworkDHCPLeasePtr {
        if index >= self.len() {
            return ptr::null_mut();
        }
        // SAFETY: `leases` was returned by libvirt with `count` elements; `index` is in bounds.
        unsafe { *self.leases.add(index) }
    }
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
            for i in 0..n {
                let lease = *self.leases.add(i);
                if !lease.is_null() {
                    sys::virNetworkDHCPLeaseFree(lease);
                }
            }
            libc::free(self.leases as *mut libc::c_void);
        }
        self.leases = ptr::null_mut();
        self.count = 0;
    }
}
