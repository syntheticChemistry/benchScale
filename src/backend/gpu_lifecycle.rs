// SPDX-License-Identifier: AGPL-3.0-only
//! GPU Device Lifecycle Management
//!
//! Provides abstractions for VFIO bind/unbind, IOMMU group discovery,
//! driver override, and PCI device management. This replaces the manual
//! `echo vfio-pci > driver_override` shell operations that were previously
//! done by hand during the Titan V campaign.

use crate::{Error, Result};
use std::fmt;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Represents a PCI device on the host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PciDevice {
    /// Bus/Device/Function in `DDDD:BB:SS.F` format
    pub bdf: String,
    /// IOMMU group number
    pub iommu_group: Option<u32>,
    /// PCI vendor ID (e.g. 0x10de for NVIDIA)
    pub vendor_id: u16,
    /// PCI device ID
    pub device_id: u16,
    /// Currently bound driver (None if unbound)
    pub driver: Option<String>,
    /// Supported reset methods for this device
    pub reset_methods: Vec<String>,
}

impl fmt::Display for PciDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{:04x}:{:04x}] driver={} iommu={}",
            self.bdf,
            self.vendor_id,
            self.device_id,
            self.driver.as_deref().unwrap_or("(none)"),
            self.iommu_group
                .map(|g| g.to_string())
                .unwrap_or_else(|| "(unknown)".to_string()),
        )
    }
}

impl PciDevice {
    /// Sysfs base path for this PCI device.
    pub fn sysfs_path(&self) -> PathBuf {
        PathBuf::from(format!("/sys/bus/pci/devices/{}", self.bdf))
    }

    /// Parse BDF string (e.g. `"0000:4d:00.0"`) into (domain, bus, slot, function).
    pub fn parse_bdf(&self) -> Option<(u16, u8, u8, u8)> {
        let parts: Vec<&str> = self.bdf.split(':').collect();
        if parts.len() != 3 {
            return None;
        }
        let domain = u16::from_str_radix(parts[0], 16).ok()?;
        let bus = u8::from_str_radix(parts[1], 16).ok()?;
        let slot_func: Vec<&str> = parts[2].split('.').collect();
        if slot_func.len() != 2 {
            return None;
        }
        let slot = u8::from_str_radix(slot_func[0], 16).ok()?;
        let function = u8::from_str_radix(slot_func[1], 16).ok()?;
        Some((domain, bus, slot, function))
    }

    /// Check whether this device supports Function Level Reset.
    pub fn supports_flr(&self) -> bool {
        self.reset_methods.iter().any(|m| m == "flr" || m == "pm")
    }
}

/// Trait for GPU / PCI device lifecycle operations.
///
/// Implementations interact with sysfs to manage VFIO bindings and
/// IOMMU group membership. This abstracts the manual operations that
/// were previously done via shell commands.
pub trait GpuLifecycle: Send + Sync {
    /// Bind a PCI device to the `vfio-pci` driver.
    ///
    /// Equivalent to:
    /// ```sh
    /// echo vfio-pci > /sys/bus/pci/devices/$BDF/driver_override
    /// echo $BDF > /sys/bus/pci/drivers_probe
    /// ```
    fn bind_vfio(&self, device: &PciDevice) -> Result<()>;

    /// Unbind a PCI device from `vfio-pci` and restore original driver.
    fn unbind_vfio(&self, device: &PciDevice) -> Result<()>;

    /// Set the driver_override for a PCI device.
    fn set_driver_override(&self, device: &PciDevice, driver: &str) -> Result<()>;

    /// Discover the IOMMU group for a given BDF.
    fn discover_iommu_group(&self, bdf: &str) -> Result<u32>;

    /// List all PCI devices in an IOMMU group.
    fn group_devices(&self, iommu_group: u32) -> Result<Vec<PciDevice>>;

    /// Check if a device supports FLR.
    fn supports_flr(&self, device: &PciDevice) -> bool {
        device.supports_flr()
    }

    /// Probe a PCI device from its BDF, reading vendor/device IDs and driver info from sysfs.
    fn probe_device(&self, bdf: &str) -> Result<PciDevice>;
}

/// Linux sysfs-based implementation of [`GpuLifecycle`].
pub struct SysfsGpuLifecycle;

impl SysfsGpuLifecycle {
    fn read_sysfs(path: &Path) -> Result<String> {
        std::fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .map_err(|e| Error::Backend(format!("Failed to read {}: {}", path.display(), e)))
    }

    fn write_sysfs(path: &Path, value: &str) -> Result<()> {
        std::fs::write(path, value)
            .map_err(|e| Error::Backend(format!("Failed to write {} to {}: {}", value, path.display(), e)))
    }

    fn read_hex_id(path: &Path) -> Result<u16> {
        let s = Self::read_sysfs(path)?;
        let s = s.trim_start_matches("0x");
        u16::from_str_radix(s, 16)
            .map_err(|e| Error::Backend(format!("Invalid hex in {}: {}", path.display(), e)))
    }
}

impl GpuLifecycle for SysfsGpuLifecycle {
    fn bind_vfio(&self, device: &PciDevice) -> Result<()> {
        info!("Binding {} to vfio-pci", device.bdf);

        // Unbind from current driver first
        if let Some(ref drv) = device.driver {
            if drv == "vfio-pci" {
                info!("{} already bound to vfio-pci", device.bdf);
                return Ok(());
            }
            let unbind_path = PathBuf::from(format!(
                "/sys/bus/pci/drivers/{}/unbind", drv
            ));
            if unbind_path.exists() {
                Self::write_sysfs(&unbind_path, &device.bdf)?;
                debug!("Unbound {} from {}", device.bdf, drv);
            }
        }

        // Set driver_override
        self.set_driver_override(device, "vfio-pci")?;

        // Trigger driver probe
        Self::write_sysfs(
            Path::new("/sys/bus/pci/drivers_probe"),
            &device.bdf,
        )?;

        info!("Successfully bound {} to vfio-pci", device.bdf);
        Ok(())
    }

    fn unbind_vfio(&self, device: &PciDevice) -> Result<()> {
        info!("Unbinding {} from vfio-pci", device.bdf);

        let unbind_path = PathBuf::from("/sys/bus/pci/drivers/vfio-pci/unbind");
        if unbind_path.exists() {
            Self::write_sysfs(&unbind_path, &device.bdf)?;
        }

        // Clear driver_override to let the kernel re-probe the original driver
        let override_path = device.sysfs_path().join("driver_override");
        Self::write_sysfs(&override_path, "")?;

        // Trigger re-probe
        Self::write_sysfs(
            Path::new("/sys/bus/pci/drivers_probe"),
            &device.bdf,
        )?;

        info!("Unbound {} from vfio-pci, original driver will re-probe", device.bdf);
        Ok(())
    }

    fn set_driver_override(&self, device: &PciDevice, driver: &str) -> Result<()> {
        let override_path = device.sysfs_path().join("driver_override");
        debug!("Setting driver_override for {} to {}", device.bdf, driver);
        Self::write_sysfs(&override_path, driver)
    }

    fn discover_iommu_group(&self, bdf: &str) -> Result<u32> {
        let iommu_link = PathBuf::from(format!("/sys/bus/pci/devices/{}/iommu_group", bdf));
        let resolved = std::fs::read_link(&iommu_link)
            .map_err(|e| Error::Backend(format!("No IOMMU group for {}: {}", bdf, e)))?;

        let group_str = resolved
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Backend(format!("Invalid IOMMU group path for {}", bdf)))?;

        group_str
            .parse::<u32>()
            .map_err(|e| Error::Backend(format!("Invalid IOMMU group number for {}: {}", bdf, e)))
    }

    fn group_devices(&self, iommu_group: u32) -> Result<Vec<PciDevice>> {
        let group_dir = PathBuf::from(format!(
            "/sys/kernel/iommu_groups/{}/devices", iommu_group
        ));

        if !group_dir.exists() {
            return Err(Error::Backend(format!("IOMMU group {} not found", iommu_group)));
        }

        let mut devices = Vec::new();
        let entries = std::fs::read_dir(&group_dir)
            .map_err(|e| Error::Backend(format!("Failed to read IOMMU group {}: {}", iommu_group, e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| Error::Backend(format!("Failed to read dir entry: {}", e)))?;
            if let Some(bdf) = entry.file_name().to_str() {
                match self.probe_device(bdf) {
                    Ok(dev) => devices.push(dev),
                    Err(e) => warn!("Failed to probe device {}: {}", bdf, e),
                }
            }
        }

        Ok(devices)
    }

    fn probe_device(&self, bdf: &str) -> Result<PciDevice> {
        let base = PathBuf::from(format!("/sys/bus/pci/devices/{}", bdf));

        if !base.exists() {
            return Err(Error::Backend(format!("PCI device {} not found in sysfs", bdf)));
        }

        let vendor_id = Self::read_hex_id(&base.join("vendor"))?;
        let device_id = Self::read_hex_id(&base.join("device"))?;

        let driver = {
            let driver_link = base.join("driver");
            if driver_link.exists() {
                std::fs::read_link(&driver_link)
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            } else {
                None
            }
        };

        let iommu_group = self.discover_iommu_group(bdf).ok();

        let reset_methods = {
            let reset_path = base.join("reset_method");
            if reset_path.exists() {
                Self::read_sysfs(&reset_path)
                    .map(|s| s.split_whitespace().map(String::from).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        Ok(PciDevice {
            bdf: bdf.to_string(),
            iommu_group,
            vendor_id,
            device_id,
            driver,
            reset_methods,
        })
    }
}

/// How a PCI device should be attached to the VM.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AttachMode {
    /// Include `<hostdev>` in domain XML before boot. Required when hot-attach
    /// fails (e.g. BARs not assigned) or when the device must be available
    /// from first boot.
    #[default]
    Cold,
    /// Hot-attach with `managed='yes'` — libvirt handles driver binding.
    HotManaged,
    /// Hot-attach with `managed='no'` — prevents FLR on detach, preserving
    /// GPU hardware state for warm-handoff.
    HotUnmanaged,
}

/// Rich VFIO passthrough configuration for a PCI device.
///
/// Replaces the thin [`PciPassthroughDevice`](crate::config_legacy::PciPassthroughDevice)
/// with full control over attach mode, ROM BAR, and arbitrary QEMU properties.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VfioPassthrough {
    /// The underlying PCI device.
    pub device: PciDevice,

    /// Whether libvirt manages driver binding (`managed='yes'` in XML).
    #[serde(default = "default_managed")]
    pub managed: bool,

    /// Enable or disable the ROM BAR. Disabling prevents VGA ROM from
    /// hanging the VM on boot for secondary GPUs.
    #[serde(default = "default_rom_bar")]
    pub rom_bar: bool,

    /// How the device should be attached to the VM.
    #[serde(default)]
    pub attach_mode: AttachMode,

    /// Arbitrary QEMU device properties (e.g. `x-no-mmap=on`).
    #[serde(default)]
    pub qemu_properties: std::collections::HashMap<String, String>,
}

fn default_managed() -> bool {
    true
}
fn default_rom_bar() -> bool {
    true
}


impl VfioPassthrough {
    /// Generate libvirt `<hostdev>` XML for this passthrough configuration.
    pub fn to_libvirt_xml(&self) -> Option<String> {
        let (domain, bus, slot, function) = self.device.parse_bdf()?;
        let managed = if self.managed { "yes" } else { "no" };

        let rom_xml = if self.rom_bar {
            ""
        } else {
            "\n      <rom bar='off'/>"
        };

        Some(format!(
            r"    <hostdev mode='subsystem' type='pci' managed='{managed}'>
      <source>
        <address domain='0x{domain:04x}' bus='0x{bus:02x}' slot='0x{slot:02x}' function='0x{function:x}'/>
      </source>{rom_xml}
    </hostdev>",
        ))
    }

    /// Convert from the legacy `PciPassthroughDevice`.
    pub fn from_legacy(legacy: &crate::config_legacy::PciPassthroughDevice) -> Self {
        let attach_mode = if legacy.no_flr {
            AttachMode::HotUnmanaged
        } else {
            AttachMode::Cold
        };

        Self {
            device: PciDevice {
                bdf: legacy.bdf.clone(),
                iommu_group: None,
                vendor_id: 0,
                device_id: 0,
                driver: None,
                reset_methods: Vec::new(),
            },
            managed: !legacy.no_flr,
            rom_bar: true,
            attach_mode,
            qemu_properties: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pci_device_display() {
        let dev = PciDevice {
            bdf: "0000:02:00.0".to_string(),
            iommu_group: Some(69),
            vendor_id: 0x10de,
            device_id: 0x1db1,
            driver: Some("nvidia".to_string()),
            reset_methods: vec!["flr".to_string(), "pm".to_string()],
        };

        let s = format!("{}", dev);
        assert!(s.contains("0000:02:00.0"));
        assert!(s.contains("10de:1db1"));
        assert!(s.contains("nvidia"));
        assert!(s.contains("69"));
    }

    #[test]
    fn test_pci_device_supports_flr() {
        let dev_with_flr = PciDevice {
            bdf: "0000:02:00.0".to_string(),
            iommu_group: None,
            vendor_id: 0x10de,
            device_id: 0x1db1,
            driver: None,
            reset_methods: vec!["flr".to_string(), "bus".to_string()],
        };
        assert!(dev_with_flr.supports_flr());

        let dev_without_flr = PciDevice {
            bdf: "0000:02:00.0".to_string(),
            iommu_group: None,
            vendor_id: 0x10de,
            device_id: 0x1db1,
            driver: None,
            reset_methods: vec!["bus".to_string()],
        };
        assert!(!dev_without_flr.supports_flr());
    }

    #[test]
    fn test_pci_device_sysfs_path() {
        let dev = PciDevice {
            bdf: "0000:4d:00.0".to_string(),
            iommu_group: None,
            vendor_id: 0,
            device_id: 0,
            driver: None,
            reset_methods: vec![],
        };
        assert_eq!(
            dev.sysfs_path(),
            PathBuf::from("/sys/bus/pci/devices/0000:4d:00.0")
        );
    }

    #[test]
    fn test_vfio_passthrough_xml_managed() {
        let pt = VfioPassthrough {
            device: PciDevice {
                bdf: "0000:02:00.0".to_string(),
                iommu_group: Some(69),
                vendor_id: 0x10de,
                device_id: 0x1db1,
                driver: None,
                reset_methods: vec![],
            },
            managed: true,
            rom_bar: true,
            attach_mode: AttachMode::Cold,
            qemu_properties: std::collections::HashMap::new(),
        };

        let xml = pt.to_libvirt_xml().unwrap();
        assert!(xml.contains("managed='yes'"));
        assert!(!xml.contains("rom bar"));
    }

    #[test]
    fn test_vfio_passthrough_xml_unmanaged_no_rom() {
        let pt = VfioPassthrough {
            device: PciDevice {
                bdf: "0000:4d:00.0".to_string(),
                iommu_group: None,
                vendor_id: 0x10de,
                device_id: 0x1db1,
                driver: None,
                reset_methods: vec![],
            },
            managed: false,
            rom_bar: false,
            attach_mode: AttachMode::HotUnmanaged,
            qemu_properties: std::collections::HashMap::new(),
        };

        let xml = pt.to_libvirt_xml().unwrap();
        assert!(xml.contains("managed='no'"));
        assert!(xml.contains("rom bar='off'"));
    }

    #[test]
    fn test_vfio_from_legacy() {
        let legacy = crate::config_legacy::PciPassthroughDevice {
            bdf: "0000:02:00.0".to_string(),
            no_flr: true,
        };
        let pt = VfioPassthrough::from_legacy(&legacy);
        assert!(!pt.managed);
        assert_eq!(pt.attach_mode, AttachMode::HotUnmanaged);
        assert_eq!(pt.device.bdf, "0000:02:00.0");
    }

    #[test]
    fn test_attach_mode_serde() {
        let json = serde_json::to_string(&AttachMode::HotManaged).unwrap();
        assert_eq!(json, r#""hot_managed""#);
        let back: AttachMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AttachMode::HotManaged);
    }
}
