use benchscale::{Backend, CloudInit, VmConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = Backend::Libvirt.connect().await?;
    
    let config = VmConfig {
        name: "popos-cosmic-final".to_string(),
        memory_mb: 4096,
        vcpus: 2,
        disk_gb: 30,
        template: Some(PathBuf::from("/var/lib/libvirt/images/popos-24-cosmic-rustdesk-template.qcow2")),
        cloud_init: None, // No cloud-init - template already configured
    };
    
    println!("Creating VM from COSMIC template...");
    let vm = backend.create_vm(&config).await?;
    
    println!("✅ VM created: {}", vm.name);
    println!("   Waiting for IP...");
    
    // Wait for IP
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
    
    if let Some(ip) = vm.ip_address().await? {
        println!("✅ IP assigned: {}", ip);
    } else {
        println!("⏳ IP not assigned yet");
    }
    
    Ok(())
}
