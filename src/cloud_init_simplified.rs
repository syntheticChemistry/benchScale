// Simplified CloudInit - Lessons from Pipeline

use crate::CloudInit;

/// Create minimal cloud-init for image building (lesson from pipeline!)
/// 
/// The pipeline taught us: simpler is better!
/// - Just SSH key
/// - Just one user
/// - Let DHCP handle networking
/// - No complex user management
pub fn minimal_cloud_init(hostname: &str, ssh_key: &str) -> CloudInit {
    let mut ci = CloudInit::builder()
        .add_user("ubuntu", ssh_key)  // Standard ubuntu user
        .build();
    // Note: hostname set via metadata, not directly on CloudInit
    ci.runcmd.push(format!("hostnamectl set-hostname {}", hostname));
    ci
}

/// Create cloud-init for desktop VMs (lesson from pipeline!)
/// 
/// What we learned:
/// - Standard packages work best
/// - Let apt handle dependencies
/// - No manual network configuration
pub fn desktop_cloud_init(hostname: &str, ssh_key: &str) -> CloudInit {
    let mut ci = CloudInit::builder()
        .add_user("ubuntu", ssh_key)
        .build();
    ci.runcmd.push(format!("hostnamectl set-hostname {}", hostname));
    ci.packages.push("ubuntu-desktop-minimal".to_string());
    ci.packages.push("pipewire".to_string());
    ci.packages.push("wireplumber".to_string());
    ci
}

/// Create cloud-init for RustDesk VMs (lesson from pipeline!)
pub fn rustdesk_cloud_init(hostname: &str, ssh_key: &str) -> CloudInit {
    let mut ci = CloudInit::builder()
        .add_user("ubuntu", ssh_key)
        .build();
    ci.runcmd.push(format!("hostnamectl set-hostname {}", hostname));
    ci.packages.push("ubuntu-desktop-minimal".to_string());
    ci.packages.push("pipewire".to_string());
    ci.packages.push("wireplumber".to_string());
    // Note: RustDesk installed separately via wget (not in repos)
    ci
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_cloud_init() {
        let ci = minimal_cloud_init("test-vm", "ssh-rsa AAAA...");
        assert!(ci.runcmd.iter().any(|cmd| cmd.contains("test-vm")));
        assert_eq!(ci.users.len(), 1);
        assert_eq!(ci.users[0].name, "ubuntu");
    }

    #[test]
    fn test_desktop_cloud_init() {
        let ci = desktop_cloud_init("desktop-vm", "ssh-rsa AAAA...");
        assert!(ci.runcmd.iter().any(|cmd| cmd.contains("desktop-vm")));
        assert!(ci.packages.iter().any(|p| p == "ubuntu-desktop-minimal"));
        assert!(ci.packages.iter().any(|p| p == "pipewire"));
    }

    #[test]
    fn test_rustdesk_cloud_init() {
        let ci = rustdesk_cloud_init("rustdesk-vm", "ssh-rsa AAAA...");
        assert!(ci.runcmd.iter().any(|cmd| cmd.contains("rustdesk-vm")));
        assert!(ci.packages.iter().any(|p| p == "ubuntu-desktop-minimal"));
    }
}

