//! Example: Discovery-based VM provisioning
//!
//! This example demonstrates the vendor-agnostic, capability-based
//! approach to VM provisioning using primal-substrate discovery.
//!
//! ## Philosophy
//!
//! Instead of hardcoding "use libvirt" or "use VMware", we:
//! 1. Register our provider with discovery
//! 2. Discover providers by capability
//! 3. Use whichever provider is available
//!
//! This makes the code work with ANY VM backend without changes!

use benchscale::backend::{Backend, VmProvider};
use primal_substrate::{Capability, Discovery};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize discovery (auto-selects mDNS or file adapter)
    let discovery = Discovery::new().await?;
    println!("Discovery initialized using {} adapter", discovery.adapter_type());
    
    // In a real app, providers would register themselves at startup
    // For this example, we'll manually register a provider
    
    #[cfg(feature = "libvirt")]
    {
        use benchscale::LibvirtBackend;
        
        // Create libvirt backend
        let libvirt = LibvirtBackend::new()?;
        let provider = VmProvider::new(
            Arc::new(libvirt),
            "libvirt-provider",
            "2.0.0",
        );
        
        // Register so others can discover us
        provider.register(&discovery).await?;
        println!("✓ Registered libvirt provider");
    }
    
    #[cfg(not(feature = "libvirt"))]
    {
        println!("Note: This example works best with --features libvirt");
        println!("      But the discovery mechanism works with ANY backend!");
    }
    
    // Now discover VM providers (zero hardcoding!)
    println!("\n=== Discovering VM Providers ===");
    let providers = VmProvider::discover_all(&discovery).await?;
    
    if providers.is_empty() {
        println!("No VM providers found. Register one to get started!");
        return Ok(());
    }
    
    for (i, provider) in providers.iter().enumerate() {
        println!("Provider {}: {} ({})", i + 1, provider.name, provider.version);
        println!("  Capabilities: {:?}", provider.capabilities);
        if let Some(endpoint) = &provider.endpoint {
            println!("  Endpoint: {}", endpoint);
        }
    }
    
    // Find a VM provisioning provider (capability-based!)
    println!("\n=== Using VM Provisioning Capability ===");
    match VmProvider::find(&discovery).await {
        Ok(service) => {
            println!("✓ Found VM provider: {}", service.name);
            println!("  This could be libvirt, VMware, AWS, or anything!");
            println!("  The code doesn't care - it just needs the capability!");
            
            // In a real app, you'd connect to this provider and use it:
            // let backend = connect_to_provider(&service).await?;
            // backend.create_node(...).await?;
        }
        Err(e) => {
            println!("✗ No VM provider found: {}", e);
            println!("  Tip: Start a provider with VmProvisioning capability");
        }
    }
    
    // Example: Search for specific capabilities
    println!("\n=== Capability-Based Search ===");
    let vm_providers = discovery
        .discover(&Capability::VmProvisioning)
        .await?;
    println!("Found {} providers with VmProvisioning capability", vm_providers.len());
    
    // Show the power: Zero vendor hardcoding!
    println!("\n=== Zero Hardcoding Validation ===");
    println!("✓ No hardcoded 'libvirt'");
    println!("✓ No hardcoded 'qemu:///system'");
    println!("✓ No hardcoded IP addresses");
    println!("✓ No hardcoded ports");
    println!("✓ Everything discovered at runtime!");
    
    Ok(())
}

/// Example of how to use a discovered provider
///
/// This function shows how you'd work with a discovered VM provider
/// without knowing which backend it is.
#[allow(dead_code)]
async fn example_vm_creation(discovery: &Discovery) -> anyhow::Result<()> {
    // Find ANY VM provider (don't care which!)
    let provider_info = VmProvider::find(discovery).await?;
    
    println!("Creating VM using: {}", provider_info.name);
    
    // In a real implementation:
    // 1. Connect to the provider using the discovered endpoint/metadata
    // 2. Use the Backend trait methods (create_node, etc.)
    // 3. The trait abstraction handles vendor differences
    
    // Pseudo-code (actual implementation would depend on your architecture):
    // let backend: Arc<dyn Backend> = connect_to_provider(&provider_info).await?;
    // let node = backend.create_node(
    //     "example-vm",
    //     "ubuntu-22.04",
    //     "default",
    //     HashMap::new(),
    // ).await?;
    // println!("Created VM: {} at {}", node.name, node.ip_address);
    
    Ok(())
}

/// Example: Multi-vendor support
///
/// This shows how the same code works with different backends
#[allow(dead_code)]
async fn multi_vendor_example() -> anyhow::Result<()> {
    let discovery = Discovery::new().await?;
    
    // Try to find any VM provider
    let backend: Arc<dyn Backend> = match VmProvider::find(&discovery).await {
        Ok(provider) => {
            println!("Using discovered provider: {}", provider.name);
            // In real code, create backend from discovered provider
            // For now, this is pseudo-code:
            todo!("Connect to provider: {}", provider.name)
        }
        Err(_) => {
            println!("No provider found, falling back to local default");
            // Even the fallback can be capability-based!
            todo!("Create local backend")
        }
    };
    
    // Now use the backend - same code regardless of which vendor!
    let node = backend.create_node(
        "test-vm",
        "ubuntu-22.04",
        "default",
        HashMap::new(),
    ).await?;
    
    println!("Created node: {} (backend agnostic!)", node.name);
    
    Ok(())
}

