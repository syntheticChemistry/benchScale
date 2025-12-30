# benchScale - Developer Guidance

> **Best practices, patterns, and guidelines for working with benchScale**

## 🎯 Quick Start for Developers

### Basic VM Creation

```rust
use benchscale::{LibvirtBackend, CloudInit};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Create backend
    let backend = LibvirtBackend::new()?;
    
    // 2. Configure cloud-init
    let cloud_init = CloudInit::builder()
        .add_user("ubuntu", "ssh-rsa AAAA...")
        .package("curl")
        .package("wget")
        .build();
    
    // 3. Create VM
    let vm = backend.create_desktop_vm(
        "my-vm",                                    // VM name
        "/var/lib/libvirt/images/ubuntu-24.04.img", // Base image
        &cloud_init,                                // Provisioning
        2048,                                       // Memory (MB)
        2,                                          // vCPUs
        20,                                         // Disk (GB)
    ).await?;
    
    println!("VM created: {} at {}", vm.name, vm.ip_address);
    
    // 4. SSH into VM
    let output = backend.ssh_exec(&vm, "ubuntu", "uname -a").await?;
    println!("SSH output: {}", output);
    
    // 5. Cleanup
    backend.delete_node(&vm.name).await?;
    
    Ok(())
}
```

## 📚 Common Patterns

### Pattern 1: VM with Static IP

```rust
use benchscale::{LibvirtBackend, CloudInit};

let backend = LibvirtBackend::new()?;

let cloud_init = CloudInit::builder()
    .add_user("ubuntu", ssh_key)
    .static_ip(
        "192.168.122.100",     // IP address
        "192.168.122.1",       // Gateway
        "255.255.255.0"        // Netmask
    )
    .build();

let vm = backend.create_desktop_vm(
    "static-vm",
    base_image,
    &cloud_init,
    2048, 2, 20
).await?;
```

### Pattern 2: VM with Custom Packages

```rust
let cloud_init = CloudInit::builder()
    .add_user("ubuntu", ssh_key)
    .package("nginx")
    .package("postgresql-14")
    .package("redis-server")
    .run_command("systemctl enable nginx")
    .run_command("systemctl start nginx")
    .build();
```

### Pattern 3: Multi-User VM

```rust
let cloud_init = CloudInit::builder()
    .add_user("admin", admin_ssh_key)
    .add_user("developer", dev_ssh_key)
    .add_user("tester", test_ssh_key)
    .package("git")
    .build();
```

### Pattern 4: Lab with Multiple VMs

```rust
use benchscale::{Lab, Topology};

// Define topology
let topology = Topology::builder()
    .subnet("192.168.122.0/24")
    .gateway("192.168.122.1")
    .vm("web", "192.168.122.10")
    .vm("db", "192.168.122.11")
    .vm("cache", "192.168.122.12")
    .build()?;

// Create lab
let lab = Lab::new("my-lab", topology)?;

// Deploy all VMs
lab.deploy_all(base_image, &cloud_init).await?;

// Access specific VM
let web_vm = lab.get_vm("web")?;
backend.ssh_exec(&web_vm, "ubuntu", "curl localhost").await?;

// Cleanup
lab.teardown().await?;
```

### Pattern 5: VM Lifecycle Management

```rust
use benchscale::{LibvirtBackend, VmState};

let backend = LibvirtBackend::new()?;

// Create VM
let vm = backend.create_desktop_vm(...).await?;

// Stop VM
backend.stop_vm(&vm.name).await?;

// Check state
let info = backend.get_node_info(&vm.name).await?;
assert_eq!(info.state, VmState::Stopped);

// Restart VM
backend.start_vm(&vm.name).await?;

// Delete VM
backend.delete_node(&vm.name).await?;
```

## 🔧 Configuration Best Practices

### 1. Use Environment Variables for Secrets

```rust
use std::env;

let ssh_key = env::var("SSH_PUBLIC_KEY")
    .expect("SSH_PUBLIC_KEY environment variable not set");

let cloud_init = CloudInit::builder()
    .add_user("ubuntu", &ssh_key)
    .build();
```

### 2. Use XDG Base Directory for Config

```rust
use benchscale::constants::{get_image_dir, get_cloud_init_tmp_dir};

// Automatically uses XDG if available, falls back to defaults
let image_dir = get_image_dir();
let tmp_dir = get_cloud_init_tmp_dir();

let base_image = image_dir.join("ubuntu-24.04.img");
```

### 3. Handle Errors Properly

```rust
use anyhow::{Context, Result};

async fn create_test_vm() -> Result<NodeInfo> {
    let backend = LibvirtBackend::new()
        .context("Failed to initialize libvirt backend")?;
    
    let vm = backend.create_desktop_vm(...)
        .await
        .context("Failed to create VM 'test-vm'")?;
    
    Ok(vm)
}
```

### 4. Clean Up Resources

```rust
use anyhow::Result;

async fn run_test() -> Result<()> {
    let backend = LibvirtBackend::new()?;
    let vm = backend.create_desktop_vm(...).await?;
    
    // Use the VM
    let result = backend.ssh_exec(&vm, "ubuntu", "echo test").await;
    
    // Always cleanup, even on error
    backend.delete_node(&vm.name).await?;
    
    result?; // Check result after cleanup
    Ok(())
}
```

## 🚫 Common Pitfalls

### ❌ Don't: Use unwrap() in Production

```rust
// BAD
let vm = backend.create_desktop_vm(...).await.unwrap();

// GOOD
let vm = backend.create_desktop_vm(...)
    .await
    .context("Failed to create VM")?;
```

### ❌ Don't: Forget to Clean Up VMs

```rust
// BAD - VM leaked if error occurs
let vm = backend.create_desktop_vm(...).await?;
// ... do work that might error ...
backend.delete_node(&vm.name).await?;

// GOOD - Always cleanup
let vm = backend.create_desktop_vm(...).await?;
let result = {
    // ... do work that might error ...
};
backend.delete_node(&vm.name).await?;
result?;
```

### ❌ Don't: Hardcode IPs

```rust
// BAD
let cloud_init = CloudInit::builder()
    .static_ip("192.168.122.10", "192.168.122.1", "255.255.255.0")
    .build();

// GOOD - Let IpPool manage it
let vm = backend.create_desktop_vm(...)
    .await?; // IP automatically allocated
```

### ❌ Don't: Block Async Code

```rust
// BAD
std::thread::sleep(Duration::from_secs(5));

// GOOD
tokio::time::sleep(Duration::from_secs(5)).await;
```

## 🧪 Testing Guidelines

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cloud_init_builder() {
        let cloud_init = CloudInit::builder()
            .add_user("test", "ssh-rsa AAA...")
            .package("vim")
            .build();
        
        assert_eq!(cloud_init.users.len(), 1);
        assert_eq!(cloud_init.packages.len(), 1);
    }
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_vm_lifecycle() -> Result<()> {
    let backend = LibvirtBackend::new()?;
    let cloud_init = CloudInit::builder()
        .add_user("ubuntu", test_ssh_key())
        .build();
    
    // Create
    let vm = backend.create_desktop_vm(
        "test-lifecycle",
        test_image_path(),
        &cloud_init,
        1024, 1, 10
    ).await?;
    
    // Verify
    assert!(backend.vm_exists(&vm.name).await?);
    
    // Cleanup
    backend.delete_node(&vm.name).await?;
    
    Ok(())
}
```

## 📖 API Reference

### Core Types

```rust
/// Information about a VM node
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub ip_address: String,
    pub state: VmState,
    pub memory_mb: u32,
    pub vcpus: u32,
}

/// VM lifecycle state
pub enum VmState {
    Creating,
    Running,
    Stopped,
    Destroyed,
}

/// Backend trait for hypervisor operations
#[async_trait]
pub trait Backend {
    async fn create_desktop_vm(...) -> Result<NodeInfo>;
    async fn start_vm(&self, name: &str) -> Result<()>;
    async fn stop_vm(&self, name: &str) -> Result<()>;
    async fn delete_node(&self, name: &str) -> Result<()>;
    async fn ssh_exec(&self, node: &NodeInfo, user: &str, cmd: &str) -> Result<String>;
}
```

### CloudInit Builder

```rust
impl CloudInit {
    pub fn builder() -> CloudInitBuilder;
}

impl CloudInitBuilder {
    pub fn add_user(self, name: &str, ssh_key: &str) -> Self;
    pub fn package(self, package: &str) -> Self;
    pub fn static_ip(self, ip: &str, gateway: &str, netmask: &str) -> Self;
    pub fn run_command(self, cmd: &str) -> Self;
    pub fn write_file(self, path: &str, content: &str) -> Self;
    pub fn build(self) -> CloudInit;
}
```

## 🔍 Debugging Tips

### Enable Debug Logging

```rust
use tracing_subscriber;

// Initialize logging
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

// Your code here
let backend = LibvirtBackend::new()?;
```

### Check VM Status

```bash
# List all VMs
virsh list --all

# Check specific VM
virsh dominfo my-vm

# View console output
virsh console my-vm
```

### Inspect Cloud-Init

```bash
# SSH into VM
ssh ubuntu@192.168.122.10

# Check cloud-init logs
sudo cat /var/log/cloud-init.log
sudo cat /var/log/cloud-init-output.log

# Check cloud-init status
cloud-init status --long
```

## 🚀 Performance Tips

### 1. Reuse Backend Instances

```rust
// GOOD - Reuse backend
let backend = LibvirtBackend::new()?;
for name in vm_names {
    backend.create_desktop_vm(name, ...).await?;
}

// BAD - Creating new backend each time
for name in vm_names {
    let backend = LibvirtBackend::new()?;
    backend.create_desktop_vm(name, ...).await?;
}
```

### 2. Parallel VM Creation

```rust
use futures::future::try_join_all;

let futures: Vec<_> = vm_names.iter()
    .map(|name| backend.create_desktop_vm(name, ...))
    .collect();

let vms = try_join_all(futures).await?;
```

### 3. Use Appropriate Timeouts

```rust
use tokio::time::{timeout, Duration};

let vm = timeout(
    Duration::from_secs(300),  // 5 minute timeout
    backend.create_desktop_vm(...)
).await??;
```

## 📚 Further Reading

- **[OVERVIEW.md](OVERVIEW.md)** - Project overview and capabilities
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Detailed architecture
- **[API.md](API.md)** - Complete API reference
- **[REFACTORING_ROADMAP.md](../REFACTORING_ROADMAP.md)** - Evolution plan

## 🤝 Contributing

When contributing to benchScale:

1. **Follow Rust best practices**
   - Use clippy: `cargo clippy`
   - Format code: `cargo fmt`
   - Add tests for new features

2. **Maintain type safety**
   - No unsafe code
   - Use proper error types
   - Leverage the type system

3. **Write documentation**
   - Document public APIs
   - Add examples for complex features
   - Update specs/ as needed

4. **Test thoroughly**
   - Add unit tests
   - Add integration tests
   - Verify with `cargo test --all-features`

---

**Happy building with benchScale!** 🦀✨

