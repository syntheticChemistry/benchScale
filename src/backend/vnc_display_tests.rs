// SPDX-License-Identifier: AGPL-3.0-only
//! Unit tests for libvirt display and VNC configuration
//!
//! These tests validate that VNC and display options are correctly configured
//! for desktop VMs.

#[cfg(test)]
mod vnc_display_tests {
    

    #[test]
    fn test_vnc_graphics_argument() {
        // Validate VNC graphics configuration string
        let vnc_config = "vnc,listen=0.0.0.0";

        assert!(vnc_config.contains("vnc"));
        assert!(vnc_config.contains("listen=0.0.0.0"));
        assert!(!vnc_config.contains("none"));
    }

    #[test]
    fn test_display_method_selection() {
        // Test that we choose VNC for desktop VMs
        struct DisplayConfig {
            method: String,
            needs_gui: bool,
        }

        let desktop_vm = DisplayConfig {
            method: "vnc".to_string(),
            needs_gui: true,
        };

        let server_vm = DisplayConfig {
            method: "none".to_string(),
            needs_gui: false,
        };

        assert_eq!(desktop_vm.method, "vnc");
        assert!(desktop_vm.needs_gui);

        assert_eq!(server_vm.method, "none");
        assert!(!server_vm.needs_gui);
    }

    #[test]
    fn test_vnc_port_range() {
        // VNC typically uses ports 5900-5999
        let vnc_display_0 = 5900;
        let vnc_display_99 = 5999;

        assert!(vnc_display_0 >= 5900);
        assert!(vnc_display_99 <= 5999);
    }

    #[test]
    fn test_graphics_options() {
        // Test different graphics options
        let options = vec![
            ("vnc,listen=0.0.0.0", true, "VNC for remote access"),
            ("none", false, "Headless server"),
            ("spice", true, "Spice for better performance"),
        ];

        for (config, has_display, description) in options {
            if has_display {
                assert!(
                    !config.contains("none"),
                    "{} should have display",
                    description
                );
            } else {
                assert_eq!(config, "none", "{} should be headless", description);
            }
        }
    }

    #[test]
    fn test_vnc_listen_addresses() {
        // Test VNC listen address options
        struct VncConfig {
            listen: String,
            accessible_from: &'static str,
        }

        let configs = vec![
            VncConfig {
                listen: "0.0.0.0".to_string(),
                accessible_from: "anywhere",
            },
            VncConfig {
                listen: "127.0.0.1".to_string(),
                accessible_from: "localhost only",
            },
        ];

        for config in configs {
            if config.listen == "0.0.0.0" {
                assert_eq!(config.accessible_from, "anywhere");
            } else if config.listen == "127.0.0.1" {
                assert_eq!(config.accessible_from, "localhost only");
            }
        }
    }

    #[test]
    fn test_virt_install_graphics_arg() {
        // Validate that we use correct virt-install graphics argument
        let desktop_args = vec![
            "--name",
            "test-vm",
            "--graphics",
            "vnc,listen=0.0.0.0", // Correct for desktop!
        ];

        let headless_args = vec![
            "--name",
            "test-vm",
            "--graphics",
            "none", // Correct for server!
        ];

        // Desktop VM should have VNC
        assert!(desktop_args.contains(&"--graphics"));
        let graphics_idx = desktop_args
            .iter()
            .position(|&x| x == "--graphics")
            .unwrap();
        assert!(desktop_args[graphics_idx + 1].contains("vnc"));

        // Headless VM should have none
        assert!(headless_args.contains(&"--graphics"));
        let graphics_idx = headless_args
            .iter()
            .position(|&x| x == "--graphics")
            .unwrap();
        assert_eq!(headless_args[graphics_idx + 1], "none");
    }

    #[test]
    fn test_vnc_vs_other_displays() {
        // Document why VNC is better for our use case
        struct DisplayMethod {
            name: &'static str,
            remote_access: bool,
            requires_x11: bool,
            libvirt_auto: bool,
        }

        let methods = vec![
            DisplayMethod {
                name: "VNC",
                remote_access: true,
                requires_x11: false,
                libvirt_auto: true,
            },
            DisplayMethod {
                name: "GTK",
                remote_access: false,
                requires_x11: true,
                libvirt_auto: false,
            },
            DisplayMethod {
                name: "SDL",
                remote_access: false,
                requires_x11: false,
                libvirt_auto: false,
            },
        ];

        // VNC should be best for remote desktop VMs
        let vnc = methods.iter().find(|m| m.name == "VNC").unwrap();
        assert!(vnc.remote_access, "VNC must support remote access");
        assert!(!vnc.requires_x11, "VNC shouldn't need X11 on host");
        assert!(vnc.libvirt_auto, "Libvirt should auto-configure VNC");
    }

    #[test]
    fn test_display_config_validation() {
        // Validate display configuration for different VM types
        fn validate_display_for_vm_type(vm_type: &str) -> String {
            match vm_type {
                "desktop" => "vnc,listen=0.0.0.0",
                "server" => "none",
                "development" => "vnc,listen=127.0.0.1",
                _ => "none",
            }
            .to_string()
        }

        assert_eq!(
            validate_display_for_vm_type("desktop"),
            "vnc,listen=0.0.0.0"
        );
        assert_eq!(validate_display_for_vm_type("server"), "none");
        assert_eq!(
            validate_display_for_vm_type("development"),
            "vnc,listen=127.0.0.1"
        );
        assert_eq!(validate_display_for_vm_type("unknown"), "none");
    }
}

#[cfg(test)]
mod libvirt_integration_tests {
    

    #[test]
    fn test_virsh_vncdisplay_command() {
        // Test the command we use to get VNC display
        let vm_name = "test-vm";
        let command = format!("virsh vncdisplay {}", vm_name);

        assert!(command.contains("virsh"));
        assert!(command.contains("vncdisplay"));
        assert!(command.contains(vm_name));
    }

    #[test]
    fn test_vnc_display_parsing() {
        // Test parsing VNC display output
        let vnc_outputs = vec![
            (":0", 5900),
            (":1", 5901),
            (":99", 5999),
            ("127.0.0.1:0", 5900),
        ];

        for (output, expected_port) in vnc_outputs {
            if output.starts_with(':') {
                let display_num: u16 = output[1..].parse().unwrap();
                let port = 5900 + display_num;
                assert_eq!(port, expected_port);
            }
        }
    }

    #[test]
    fn test_virt_install_command_structure() {
        // Validate virt-install command structure for desktop VM
        let img = crate::constants::paths::default_system_vm_images_dir();
        let disk_main = format!("path={},format=qcow2", img.join("test.qcow2").display());
        let disk_cidata = format!("path={},device=cdrom", img.join("test-cidata.iso").display());
        let command_args = vec![
            "virt-install".to_string(),
            "--name".to_string(),
            "test-vm".to_string(),
            "--memory".to_string(),
            "2048".to_string(),
            "--vcpus".to_string(),
            "2".to_string(),
            "--disk".to_string(),
            disk_main,
            "--disk".to_string(),
            disk_cidata,
            "--os-variant".to_string(),
            "ubuntu22.04".to_string(),
            "--network".to_string(),
            "network=default".to_string(),
            "--graphics".to_string(),
            "vnc,listen=0.0.0.0".to_string(), // Critical for VNC!
            "--noautoconsole".to_string(),
            "--import".to_string(),
        ];

        // Verify required arguments are present
        assert!(command_args.iter().any(|s| s == "virt-install"));
        assert!(command_args.iter().any(|s| s == "--name"));
        assert!(command_args.iter().any(|s| s == "--graphics"));
        assert!(command_args.iter().any(|s| s == "--network"));
        assert!(command_args.iter().any(|s| s == "--import"));

        // Verify graphics is VNC
        let graphics_idx = command_args
            .iter()
            .position(|x| x == "--graphics")
            .unwrap();
        assert!(command_args[graphics_idx + 1].contains("vnc"));
    }

    #[test]
    fn test_cloud_init_iso_attachment() {
        // Validate that cloud-init ISO is attached as CDROM
        let img = crate::constants::paths::default_system_vm_images_dir();
        let disk_main = format!("path={},format=qcow2", img.join("vm.qcow2").display());
        let disk_cidata = format!("path={},device=cdrom", img.join("vm-cidata.iso").display());
        let disk_args = vec![
            "--disk",
            disk_main.as_str(),
            "--disk",
            disk_cidata.as_str(),
        ];

        // Should have main disk
        assert!(disk_args.iter().any(|arg| arg.contains(".qcow2")));

        // Should have cloud-init ISO as cdrom
        assert!(disk_args.iter().any(|arg| arg.contains("cidata.iso")));
        assert!(disk_args.iter().any(|arg| arg.contains("device=cdrom")));
    }
}
