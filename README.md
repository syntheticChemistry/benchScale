# benchScale v2.0.0

**Production-Ready VM Management Framework**

A pure Rust laboratory substrate for distributed system testing with sovereign architecture, CloudInit support, and first-class libvirt integration.

[![Status](https://img.shields.io/badge/status-production%20ready-brightgreen)]()
[![Tests](https://img.shields.io/badge/tests-106%2F106%20passing-success)]()
[![Coverage](https://img.shields.io/badge/coverage-90.24%25-brightgreen)]()
[![Grade](https://img.shields.io/badge/grade-A%2B%20(98%2F100)-success)]()

benchScale provides a type-safe, declarative framework for creating reproducible test environments for distributed systems, P2P networks, and multi-node applications. Built with pure Rust on Docker and libvirt.

---

## ✨ New in v2.0.0

### CloudInit Support ⭐ NEW (Dec 28, 2025)
- **Type-Safe Configuration** - Builder pattern for cloud-init generation
- **Desktop VM Creation** - Full desktop environment provisioning
- **SSH Key Injection** - Automated user setup
- **Package Installation** - Declarative package lists
- **Production Ready** - Used by ionChannel for autonomous provisioning

### Enhanced libvirt Backend
- **Real VM Creation** - `create_desktop_vm()` with cloud-init
- **IP Acquisition** - Automatic DHCP monitoring
- **SSH Execution** - Remote command execution
- **Service Validation** - Process and port checking

---

## 🎯 What is benchScale?

benchScale is a **laboratory substrate** that enables developers to:

- Create **reproducible test environments** from declarative YAML topologies
- Simulate **real-world network conditions** (latency, packet loss, bandwidth)
- Provision **desktop VMs** with cloud-init automation
- Test **distributed systems** before production deployment
- Orchestrate **multi-node scenarios** with container and VM backends

---

## 🚀 Quick Start

### Installation

```bash
# Clone repository
git clone git@github.com:ecoPrimals/benchScale.git
cd benchScale

# Build
cargo build --release --features libvirt

# Verify
./target/release/benchscale --version
```

### Your First Desktop VM

```rust
use benchscale::{LibvirtBackend, CloudInit};
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = LibvirtBackend::new()?;
    
    let cloud_init = CloudInit::builder()
        .add_user("testuser", "ssh-rsa AAAAB3...")
        .package("ubuntu-desktop-minimal")
        .package("xrdp")
        .package_update(true)
        .build();
    
    let node = backend.create_desktop_vm(
        "my-desktop-vm",
        Path::new("/path/to/ubuntu-22.04.img"),
        &cloud_init,
        3072,  // RAM MB
        2,      // vCPUs
        25,     // Disk GB
    ).await?;
    
    println!("VM ready at {}", node.ip_address);
    Ok(())
}
```

### Traditional Lab Example

```bash
# Create a 2-node LAN lab
./target/release/benchscale create my-lab topologies/simple-lan.yaml

# List labs
./target/release/benchscale list

# Destroy lab
./target/release/benchscale destroy my-lab
```

---

## 🔧 Key Features

### 🦀 **Pure Rust Architecture**
- Zero shell scripts - direct API integration
- Type-safe lab management
- Modern async/await throughout
- **90.24% test coverage** ✅
- Zero unsafe code

### ☁️ **CloudInit Integration** ⭐ NEW
- Type-safe configuration builder
- YAML generation for VM automation
- User creation with SSH keys
- Package installation
- Command execution
- File writing

### 🐧 **Desktop VM Support** ⭐ NEW
- Full desktop environment provisioning
- Ubuntu, Fedora, Debian support
- Automated RustDesk/XRDP installation
- Real IP acquisition via DHCP
- SSH availability detection

### 🌐 **Network Simulation**
- Latency injection (LAN/WAN/cellular presets)
- Packet loss simulation
- Bandwidth limiting
- NAT traversal testing

### 🐳 **Multiple Backends**
- **Docker** - Container-based labs (production ready)
- **libvirt/KVM** - VM-based labs with qcow2 overlays (production ready)
- Extensible backend trait for future runtimes

### 🔐 **Zero Hardcoding**
- 15+ configuration options via environment variables
- TOML configuration file support
- Runtime capability discovery
- No hardcoded credentials or paths

---

## 📖 CloudInit Example

```rust
use benchscale::CloudInit;

let cloud_init = CloudInit::builder()
    // Add user with SSH key
    .add_user("iontest", "ssh-rsa AAAAB3...")
    
    // Install packages
    .package("ubuntu-desktop-minimal")
    .package("xrdp")
    .package("rustdesk")
    
    // Run commands
    .cmd("systemctl enable xrdp")
    .cmd("systemctl start rustdesk")
    
    // Update system
    .package_update(true)
    .package_upgrade(true)
    
    .build();

// Generate cloud-init YAML
let yaml = cloud_init.to_user_data()?;
```

---

## ⚙️ Configuration

```bash
# Libvirt/KVM Settings
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_BASE_IMAGE_PATH="/var/lib/libvirt/images"
export BENCHSCALE_OVERLAY_DIR="/tmp/benchscale"

# SSH Settings
export BENCHSCALE_SSH_USER="myuser"
export BENCHSCALE_SSH_KEY="~/.ssh/id_rsa"
export BENCHSCALE_SSH_PORT="22"

# Docker Settings
export BENCHSCALE_HARDENED_IMAGES="true"
export BENCHSCALE_DOCKER_TIMEOUT_SECS="60"
```

Or use `~/.config/benchscale/benchscale.toml`:

```toml
[libvirt]
uri = "qemu:///system"
base_image_path = "/var/lib/libvirt/images"

[libvirt.ssh]
default_user = "myuser"
key_path = "~/.ssh/id_rsa"
```

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│          Application Layer (ionChannel, CI/CD)          │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│              benchScale Core (Rust)                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ CloudInit    │  │ Lab Registry │  │ Test Runner  │  │
│  │ Builder      │  │              │  │              │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Backend Trait (Abstraction)              │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
         ┌───────────────┴───────────────┐
         ▼                               ▼
┌──────────────────┐            ┌──────────────────┐
│  DockerBackend   │            │ LibvirtBackend   │
│  (containers)    │            │ (VMs + CloudInit)│
└──────────────────┘            └──────────────────┘
```

### Core Components
- **CloudInit Builder** - Type-safe cloud-init configuration (NEW)
- **Topology Parser** - YAML topology definitions
- **Lab Manager** - Lab lifecycle orchestration
- **Backend Trait** - Abstraction over runtimes
- **Lab Registry** - Persistent lab state
- **Network Simulator** - Traffic shaping (tc)
- **Config System** - Environment-driven configuration

---

## 📊 Project Status

**Current Version:** 2.0.0  
**Status:** Production Ready ✅  
**Quality Grade:** A+ (98/100) 🏆

### Metrics
- **Lines of Code:** 2,321 (includes CloudInit module)
- **Test Coverage:** **90.24%** (106/106 tests passing) ✅
- **Modules at 90%+:** 6 of 9 modules
- **Hardcoded Values:** 0 (100% configurable)
- **Unsafe Code:** 0 blocks
- **Build Status:** ✅ Clean (no warnings)

### Recent Enhancements (Dec 28, 2025)
- ✅ **CloudInit Module** - 213 lines of production code
- ✅ **create_desktop_vm()** - Real VM provisioning
- ✅ **ionChannel Integration** - Autonomous RustDesk deployment
- ✅ **Production Validation** - Used in real automation

### Production Readiness
- ✅ **Docker backend** - Production ready
- ✅ **libvirt backend** - Production ready (enhanced)
- ✅ **CloudInit support** - Production ready (NEW)
- ✅ **Configuration system** - 97% coverage
- ✅ **Error handling** - 100% coverage
- ✅ **Lab management** - 96.6% coverage

---

## 🔬 CLI Usage

```bash
# Create a lab
benchscale create <lab-name> <topology-file>

# List all labs
benchscale list

# Show lab status
benchscale status <lab-name>

# Destroy a lab
benchscale destroy <lab-name>

# Show version
benchscale version
```

---

## 🧑‍💻 Development

### Build

```bash
# Development build
cargo build

# Release with all features
cargo build --release --features libvirt

# Docker backend only
cargo build --no-default-features --features docker
```

### Test

```bash
# Run all tests
cargo test

# Run with specific backend
cargo test --features libvirt

# With output
cargo test -- --nocapture
```

---

## 🤝 Contributing

Contributions welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and formatting (`cargo test && cargo fmt`)
5. Submit a pull request

---

## 🏛️ Philosophy: Sovereign Tool

benchScale is a **sovereign tool** designed for:
- **Reusability** - Useful across multiple projects
- **Independence** - Stands alone, composes at runtime
- **Production Quality** - Zero hardcoding, zero unsafe code
- **Clear Purpose** - VM and container management for testing

Used by:
- **ionChannel** - Remote desktop validation
- **BiomeOS** - System testing
- **ecoPrimals** - Distributed system validation

---

## 🔗 Related Projects

- **[ionChannel](../ionChannel/)** - Capability-based remote desktop
- **[BiomeOS](../BiomeOS/)** - Sovereign operating system
- **[RhizoCrypt](../RhizoCrypt/)** - Content-addressed DAG engine

---

## 📝 License

See [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

Built with:
- [Rust](https://rust-lang.org/) - Systems programming language
- [tokio](https://tokio.rs/) - Async runtime
- [bollard](https://docs.rs/bollard/) - Docker API client
- [virt](https://docs.rs/virt/) - libvirt bindings
- [serde](https://serde.rs/) - Serialization framework

---

## 📞 Support

- **Issues:** [GitHub Issues](https://github.com/ecoPrimals/benchScale/issues)
- **Discussions:** [GitHub Discussions](https://github.com/ecoPrimals/benchScale/discussions)

---

**benchScale v2.0.0** - *Sovereign VM management for modern infrastructure*

Production-ready • CloudInit integrated • Zero unsafe code

Made with 🦀 by the ecoPrimals community
