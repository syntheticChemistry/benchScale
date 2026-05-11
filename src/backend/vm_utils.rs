// SPDX-License-Identifier: AGPL-3.0-only
//! VM disk management utilities for LibvirtBackend
//!
//! Handles qcow2 disk image operations including copy-on-write overlays.

use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info};

use crate::{Error, Result};

/// Disk image manager for VM backends
pub struct DiskManager<'a> {
    overlay_dir: &'a Path,
}

impl<'a> DiskManager<'a> {
    /// Create a new disk manager with specified overlay directory
    pub fn new(overlay_dir: &'a Path) -> Self {
        Self { overlay_dir }
    }

    /// Create a copy-on-write disk overlay from a base image
    ///
    /// This creates a qcow2 overlay that uses the base image as backing storage,
    /// allowing fast VM creation without copying the entire disk.
    pub async fn create_overlay(&self, base_image: &Path, vm_name: &str) -> Result<PathBuf> {
        // Ensure overlay directory exists
        tokio::fs::create_dir_all(self.overlay_dir).await?;

        let mut overlay_path = self.overlay_dir.to_path_buf();
        overlay_path.push(format!("{}.qcow2", vm_name));

        info!(
            "Creating disk overlay: {} (base: {})",
            overlay_path.display(),
            base_image.display()
        );

        // qemu-img create -f qcow2 -b base.qcow2 -F qcow2 overlay.qcow2
        let output = Command::new("qemu-img")
            .args(["create", "-f", "qcow2", "-b"])
            .arg(base_image)
            .args(["-F", "qcow2"])
            .arg(&overlay_path)
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to run qemu-img: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Backend(format!(
                "Failed to create disk overlay: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        debug!(
            "Disk overlay created successfully: {}",
            overlay_path.display()
        );
        Ok(overlay_path)
    }

    /// Delete a disk overlay
    pub async fn delete_overlay(&self, vm_name: &str) -> Result<()> {
        let mut overlay_path = self.overlay_dir.to_path_buf();
        overlay_path.push(format!("{}.qcow2", vm_name));

        if overlay_path.exists() {
            info!("Deleting disk overlay: {}", overlay_path.display());
            tokio::fs::remove_file(&overlay_path).await?;
        }

        Ok(())
    }

    /// Check if qemu-img is available
    pub async fn is_available() -> bool {
        Command::new("qemu-img")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Parse memory string (e.g., "2G", "512M", "2048") to megabytes
pub fn parse_memory(mem_str: &str) -> Option<u32> {
    let mem_str = mem_str.trim().to_uppercase();

    if let Some(num) = mem_str.strip_suffix('G') {
        num.parse::<u32>().ok().map(|n| n * 1024)
    } else if let Some(num) = mem_str.strip_suffix('M') {
        num.parse().ok()
    } else {
        // Assume megabytes if no suffix
        mem_str.parse().ok()
    }
}

/// Generate libvirt domain XML for a VM
///
/// When `pci_devices` is non-empty, `<hostdev>` elements are added for
/// VFIO PCI passthrough. Devices must be bound to `vfio-pci` on the host.
pub fn generate_domain_xml(
    name: &str,
    disk_path: &Path,
    memory_mb: u32,
    vcpus: u32,
    network: &str,
    serial_log: &Path,
) -> String {
    generate_domain_xml_with_pci(name, disk_path, memory_mb, vcpus, network, serial_log, &[])
}

/// Generate libvirt domain XML with optional PCI passthrough devices
pub fn generate_domain_xml_with_pci(
    name: &str,
    disk_path: &Path,
    memory_mb: u32,
    vcpus: u32,
    network: &str,
    serial_log: &Path,
    pci_devices: &[crate::backend::gpu_lifecycle::VfioPassthrough],
) -> String {
    let hostdev_xml: String = pci_devices
        .iter()
        .filter_map(|d| d.to_libvirt_xml())
        .collect::<Vec<_>>()
        .join("\n");

    let hostdev_section = if hostdev_xml.is_empty() {
        String::new()
    } else {
        format!("\n{hostdev_xml}")
    };

    format!(
        r"<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory}</memory>
  <vcpu>{vcpus}</vcpu>
  <os>
    <type arch='x86_64'>hvm</type>
    <boot dev='hd'/>
  </os>
  <features>
    <acpi/>
    <apic/>
  </features>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
    <interface type='network'>
      <source network='{network}'/>
      <model type='virtio'/>
    </interface>
    <serial type='file'>
      <source path='{serial_log}'/>
      <target type='isa-serial' port='0'/>
    </serial>
    <console type='file'>
      <source path='{serial_log}'/>
      <target type='serial' port='0'/>
    </console>
    <graphics type='none'/>{hostdev}
  </devices>
</domain>",
        name = name,
        memory = memory_mb,
        vcpus = vcpus,
        disk = disk_path.display(),
        network = network,
        serial_log = serial_log.display(),
        hostdev = hostdev_section,
    )
}

/// Configuration for desktop/GUI domain XML generation.
///
/// Used by [`generate_desktop_domain_xml`] to produce a libvirt domain XML
/// that includes VNC graphics, optional cloud-init CD-ROM, MAC address,
/// and PCI passthrough devices. Replaces the `virt-install` shell-out.
pub struct DesktopDomainConfig<'a> {
    /// VM name
    pub name: &'a str,
    /// Path to the primary qcow2 disk image
    pub disk_path: &'a Path,
    /// Optional cloud-init ISO to attach as CD-ROM
    pub cdrom_path: Option<&'a Path>,
    /// Memory in MiB
    pub memory_mb: u32,
    /// Number of virtual CPUs
    pub vcpus: u32,
    /// Libvirt network name (e.g. "default")
    pub network: &'a str,
    /// Deterministic MAC address for DHCP discovery
    pub mac_address: Option<&'a str>,
    /// PCI devices for VFIO passthrough. Only `Cold` attach-mode devices are
    /// included in the domain XML; `Hot*` devices must be attached after boot
    /// via `Domain::attach_device_flags`.
    pub pci_devices: &'a [crate::backend::gpu_lifecycle::VfioPassthrough],
    /// QEMU emulator binary path. Defaults to `/usr/bin/qemu-system-x86_64`.
    pub emulator: Option<&'a str>,
}

/// Generate a libvirt domain XML for a desktop VM with VNC graphics.
///
/// This replaces the `virt-install` shell-out, giving us full control
/// over the domain definition without Python/virt-install version fragility
/// or hardcoded `--os-variant`.
pub fn generate_desktop_domain_xml(config: &DesktopDomainConfig<'_>) -> String {
    let cdrom_xml = config
        .cdrom_path
        .map(|p| {
            format!(
                r#"
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='{}'/>
      <target dev='sda' bus='sata'/>
      <readonly/>
    </disk>"#,
                p.display()
            )
        })
        .unwrap_or_default();

    let mac_xml = config
        .mac_address
        .map(|mac| format!("\n      <mac address='{}'/>", mac))
        .unwrap_or_default();

    // Only include Cold-attach devices in domain XML;
    // Hot-attach devices are attached post-boot via Domain::attach_device_flags
    let hostdev_xml: String = config
        .pci_devices
        .iter()
        .filter(|d| d.attach_mode == crate::backend::gpu_lifecycle::AttachMode::Cold)
        .filter_map(|d| d.to_libvirt_xml())
        .collect::<Vec<_>>()
        .join("\n");

    let hostdev_section = if hostdev_xml.is_empty() {
        String::new()
    } else {
        format!("\n{hostdev_xml}")
    };

    let emulator = config
        .emulator
        .unwrap_or("/usr/bin/qemu-system-x86_64");

    // Collect QEMU device properties from all PCI devices into
    // <qemu:commandline> arguments. Properties become -set device.hostN.key=val.
    let qemu_args: Vec<String> = config
        .pci_devices
        .iter()
        .filter(|d| d.attach_mode == crate::backend::gpu_lifecycle::AttachMode::Cold)
        .enumerate()
        .flat_map(|(idx, d)| {
            d.qemu_properties.iter().map(move |(k, v)| {
                format!(
                    "  <qemu:arg value='-set'/>\n  <qemu:arg value='device.hostdev{}.{k}={v}'/>",
                    idx
                )
            })
        })
        .collect();

    let qemu_ns = if qemu_args.is_empty() {
        ("", String::new())
    } else {
        (
            " xmlns:qemu='http://libvirt.org/schemas/domain/qemu/1.0'",
            format!(
                "\n<qemu:commandline>\n{}\n</qemu:commandline>",
                qemu_args.join("\n")
            ),
        )
    };

    format!(
        r#"<domain type='kvm'{qemu_xmlns}>
  <name>{name}</name>
  <memory unit='MiB'>{memory}</memory>
  <vcpu>{vcpus}</vcpu>
  <os>
    <type arch='x86_64'>hvm</type>
    <boot dev='hd'/>
  </os>
  <features>
    <acpi/>
    <apic/>
  </features>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <emulator>{emulator}</emulator>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>{cdrom}
    <interface type='network'>
      <source network='{network}'/>{mac}
      <model type='virtio'/>
    </interface>
    <graphics type='vnc' port='-1' autoport='yes' listen='0.0.0.0'/>
    <video>
      <model type='virtio'/>
    </video>{hostdev}
  </devices>{qemu_cmd}
</domain>"#,
        qemu_xmlns = qemu_ns.0,
        name = config.name,
        memory = config.memory_mb,
        vcpus = config.vcpus,
        disk = config.disk_path.display(),
        cdrom = cdrom_xml,
        network = config.network,
        mac = mac_xml,
        hostdev = hostdev_section,
        emulator = emulator,
        qemu_cmd = qemu_ns.1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_memory("2G"), Some(2048));
        assert_eq!(parse_memory("512M"), Some(512));
        assert_eq!(parse_memory("2048"), Some(2048));
        assert_eq!(parse_memory("4g"), Some(4096));
        assert_eq!(parse_memory("  1G  "), Some(1024));
        assert_eq!(parse_memory("invalid"), None);
    }

    #[test]
    fn test_generate_domain_xml() {
        let xml = generate_domain_xml(
            "test-vm",
            Path::new("/tmp/disk.qcow2"),
            2048,
            2,
            "test-net",
            Path::new("/tmp/serial.log"),
        );

        assert!(xml.contains("<name>test-vm</name>"));
        assert!(xml.contains("<memory unit='MiB'>2048</memory>"));
        assert!(xml.contains("<vcpu>2</vcpu>"));
        assert!(xml.contains("source file='/tmp/disk.qcow2'"));
        assert!(xml.contains("source network='test-net'"));
        assert!(xml.contains("source path='/tmp/serial.log'"));
    }

    #[test]
    fn test_generate_desktop_domain_xml_basic() {
        let config = DesktopDomainConfig {
            name: "desktop-vm",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: None,
            memory_mb: 4096,
            vcpus: 4,
            network: "default",
            mac_address: None,
            pci_devices: &[],
            emulator: None,
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(xml.contains("<name>desktop-vm</name>"));
        assert!(xml.contains("<memory unit='MiB'>4096</memory>"));
        assert!(xml.contains("<vcpu>4</vcpu>"));
        assert!(xml.contains("type='vnc'"));
        assert!(!xml.contains("device='cdrom'"));
        assert!(!xml.contains("<mac"));
    }

    #[test]
    fn test_generate_desktop_domain_xml_with_cdrom_and_mac() {
        let config = DesktopDomainConfig {
            name: "test-vm",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: Some(Path::new("/tmp/cidata.iso")),
            memory_mb: 2048,
            vcpus: 2,
            network: "default",
            mac_address: Some("52:54:00:ab:cd:ef"),
            pci_devices: &[],
            emulator: None,
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(xml.contains("device='cdrom'"));
        assert!(xml.contains("/tmp/cidata.iso"));
        assert!(xml.contains("52:54:00:ab:cd:ef"));
    }

    #[test]
    fn test_generate_desktop_domain_xml_filters_by_attach_mode() {
        use crate::backend::gpu_lifecycle::{AttachMode, PciDevice, VfioPassthrough};

        let devices = vec![
            VfioPassthrough {
                device: PciDevice {
                    bdf: "0000:02:00.0".to_string(),
                    iommu_group: None,
                    vendor_id: 0x10de,
                    device_id: 0x1db1,
                    driver: None,
                    reset_methods: vec![],
                },
                managed: true,
                rom_bar: true,
                attach_mode: AttachMode::Cold,
                qemu_properties: Default::default(),
            },
            VfioPassthrough {
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
                qemu_properties: Default::default(),
            },
        ];

        let config = DesktopDomainConfig {
            name: "gpu-vm",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: None,
            memory_mb: 8192,
            vcpus: 8,
            network: "default",
            mac_address: None,
            pci_devices: &devices,
            emulator: None,
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(xml.contains("0x02"), "cold-attach device should be in XML");
        assert!(!xml.contains("0x4d"), "hot-attach device should NOT be in XML");
    }

    #[test]
    fn test_qemu_commandline_injection() {
        use crate::backend::gpu_lifecycle::{AttachMode, PciDevice, VfioPassthrough};

        let mut props = std::collections::HashMap::new();
        props.insert("x-no-mmap".to_string(), "on".to_string());

        let devices = vec![VfioPassthrough {
            device: PciDevice {
                bdf: "0000:02:00.0".to_string(),
                iommu_group: None,
                vendor_id: 0x10de,
                device_id: 0x1db1,
                driver: None,
                reset_methods: vec![],
            },
            managed: false,
            rom_bar: false,
            attach_mode: AttachMode::Cold,
            qemu_properties: props,
        }];

        let config = DesktopDomainConfig {
            name: "qemu-props-vm",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: None,
            memory_mb: 8192,
            vcpus: 4,
            network: "default",
            mac_address: None,
            pci_devices: &devices,
            emulator: None,
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(xml.contains("xmlns:qemu="), "should have QEMU namespace");
        assert!(xml.contains("<qemu:commandline>"), "should have commandline block");
        assert!(xml.contains("x-no-mmap=on"), "should inject QEMU property");
    }

    #[test]
    fn test_no_qemu_commandline_without_properties() {
        let config = DesktopDomainConfig {
            name: "no-props",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: None,
            memory_mb: 2048,
            vcpus: 2,
            network: "default",
            mac_address: None,
            pci_devices: &[],
            emulator: None,
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(!xml.contains("xmlns:qemu"), "should NOT have QEMU namespace without properties");
        assert!(!xml.contains("<qemu:commandline>"), "should NOT have commandline block");
    }

    #[test]
    fn test_custom_emulator_path() {
        let config = DesktopDomainConfig {
            name: "custom-emu",
            disk_path: Path::new("/tmp/disk.qcow2"),
            cdrom_path: None,
            memory_mb: 2048,
            vcpus: 2,
            network: "default",
            mac_address: None,
            pci_devices: &[],
            emulator: Some("/usr/local/bin/qemu-system-x86_64"),
        };

        let xml = generate_desktop_domain_xml(&config);
        assert!(xml.contains("/usr/local/bin/qemu-system-x86_64"), "should use custom emulator");
    }
}
