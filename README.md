# benchScale v2.0.0

**A Pure Rust Laboratory Substrate for Distributed System Testing**

[![Status](https://img.shields.io/badge/status-production%20ready-brightgreen)](COVERAGE_GOAL_ACHIEVED.md)
[![Grade](https://img.shields.io/badge/grade-A%2B%20(98%2F100)-success)](COVERAGE_GOAL_ACHIEVED.md)
[![Tests](https://img.shields.io/badge/tests-106%2F106%20passing-success)](COVERAGE_GOAL_ACHIEVED.md)
[![Coverage](https://img.shields.io/badge/coverage-90.24%25-brightgreen)](COVERAGE_GOAL_ACHIEVED.md)
[![Build](https://img.shields.io/badge/build-passing-success)](COVERAGE_GOAL_ACHIEVED.md)

benchScale provides a type-safe, declarative framework for creating reproducible test environments for distributed systems, P2P networks, and multi-node applications. Built with pure Rust on Docker and libvirt with first-class BiomeOS support.

---

## 🎯 What is benchScale?

benchScale is a **laboratory substrate** that enables developers and operators to:

- Create **reproducible test environments** from declarative YAML topologies
- Simulate **real-world network conditions** (latency, packet loss, bandwidth, NAT)
- Test **distributed systems** before production deployment
- Orchestrate **multi-node scenarios** with container and VM backends
- Integrate seamlessly with **BiomeOS** for sovereign infrastructure testing

---

## ✨ Key Features

### 🔧 **Pure Rust Architecture**
- Zero shell scripts - direct API integration
- Type-safe lab management
- Modern async/await throughout
- Comprehensive error handling
- **90.24% test coverage** ✅

### 🌐 **Network Simulation**
- Latency injection (LAN/WAN/cellular presets)
- Packet loss simulation
- Bandwidth limiting
- NAT traversal testing

### 🐳 **Multiple Backends**
- **Docker** - Container-based labs (production ready)
- **libvirt/KVM** - VM-based labs with qcow2 overlays (beta)
- Extensible backend trait for future runtimes

### 🔐 **Zero Hardcoding**
- 15+ configuration options via environment variables
- TOML configuration file support
- Runtime capability discovery
- No hardcoded credentials or paths

### 🏥 **VM Health Monitoring**
- Boot completion detection
- Serial console capture (BiomeOS BootLogger)
- Network reachability checks
- Real-time health status

### 💾 **Lab Persistence**
- Registry for managing lab state
- List/load/delete operations
- Persistent across CLI sessions

---

## 🚀 Quick Start

### Prerequisites

```bash
# Docker (required)
curl -fsSL https://get.docker.com | sh
docker ps  # Verify running

# libvirt/KVM (optional, for VM backends)
sudo apt install qemu-kvm libvirt-daemon-system
```

### Installation

```bash
# Clone repository
git clone git@github.com:ecoPrimals/benchScale.git
cd benchScale

# Build
cargo build --release

# Verify
./target/release/benchscale --version
# benchScale v2.0.0
```

### Your First Lab

```bash
# Create a 2-node LAN lab
./target/release/benchscale create my-lab topologies/simple-lan.yaml

# List labs
./target/release/benchscale list

# Show lab status
./target/release/benchscale status my-lab

# Destroy lab
./target/release/benchscale destroy my-lab
```

---

## 📖 Documentation

### Getting Started
- **[Quick Start Guide](QUICKSTART.md)** - Quick examples and workflows
- **[Coverage Report](COVERAGE_GOAL_ACHIEVED.md)** - 90% coverage achievement 🏆
- **[Configuration Guide](#configuration)** - Environment variables and settings

### Technical
- **[Technical Specification](specs/SPECIFICATION.md)** - Complete architecture and API reference
- **[Development Status](specs/DEVELOPMENT_STATUS.md)** - Current implementation status
- **[BiomeOS Integration](BIOMEOS_INTEGRATION.md)** - BiomeOS-specific features

### Reference
- **[Documentation Index](DOCUMENTATION_INDEX.md)** - Complete documentation navigation
- **[API Documentation](#)** - Generated docs (run `cargo doc --open`)

### Quality Reports
- **[Coverage Achievement](COVERAGE_GOAL_ACHIEVED.md)** - 90.24% coverage milestone 🏆
- **[Phase Reports](.)** - Coverage milestones and progress tracking

---

## ⚙️ Configuration

benchScale uses environment-driven configuration with sensible defaults:

```bash
# Libvirt/KVM Settings
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_BASE_IMAGE_PATH="/var/lib/libvirt/images"
export BENCHSCALE_OVERLAY_DIR="/tmp/benchscale"

# SSH Settings (for VM backends)
export BENCHSCALE_SSH_USER="myuser"
export BENCHSCALE_SSH_KEY="~/.ssh/id_rsa"
export BENCHSCALE_SSH_PORT="22"
export BENCHSCALE_SSH_TIMEOUT_SECS="30"

# Docker Settings
export BENCHSCALE_HARDENED_IMAGES="true"
export BENCHSCALE_DOCKER_TIMEOUT_SECS="60"

# Lab Settings
export BENCHSCALE_STATE_DIR="/var/lib/benchscale"
export BENCHSCALE_DEFAULT_NETWORK_BRIDGE="br0"
```

Or use a TOML config file at `~/.config/benchscale/benchscale.toml`:

```toml
[libvirt]
uri = "qemu:///system"
base_image_path = "/var/lib/libvirt/images"
overlay_dir = "/tmp/benchscale"

[libvirt.ssh]
default_user = "myuser"
key_path = "~/.ssh/id_rsa"
port = 22
timeout_secs = 30

[docker]
use_hardened_images = true
timeout_secs = 60

[lab]
state_dir = "/var/lib/benchscale"
default_network_bridge = "br0"
```

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                Application Layer                         │
│    (BiomeOS, CLI, Test Scripts, CI/CD Integration)     │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│                  benchScale Core (Rust)                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Topology   │  │ Lab Registry │  │ Test Runner  │  │
│  │   Parser     │  │              │  │              │  │
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
│  (containers)    │            │ (VMs + overlays) │
└──────────────────┘            └──────────────────┘
```

### Core Components

- **Topology Parser** - YAML topology definitions
- **Lab Manager** - Lab lifecycle orchestration
- **Backend Trait** - Abstraction over runtimes
- **Lab Registry** - Persistent lab state
- **Network Simulator** - Traffic shaping (tc)
- **Health Monitor** - VM boot and readiness checks
- **Config System** - Environment-driven configuration

---

## 🧪 Example Topologies

### Simple 2-Node LAN

```yaml
metadata:
  name: simple-lan
  description: Two nodes on a fast LAN

network:
  name: simple-lan
  subnet: 10.42.0.0/24
  conditions:
    latency_ms: 1
    packet_loss_percent: 0.0

nodes:
  - name: node-1
    image: alpine:latest
  - name: node-2
    image: alpine:latest
```

### P2P with NAT Traversal

```yaml
metadata:
  name: p2p-nat
  description: Peer-to-peer with NAT simulation

network:
  name: p2p-nat
  subnet: 10.42.0.0/24

nodes:
  - name: peer-1
    image: alpine:latest
    network_conditions:
      nat_type: symmetric
  - name: peer-2
    image: alpine:latest
    network_conditions:
      nat_type: cone
  - name: relay
    image: alpine:latest
```

More examples in [`topologies/`](topologies/).

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

# Show help
benchscale help
```

---

## 🧑‍💻 Development

### Build from Source

```bash
# Development build
cargo build

# Release build
cargo build --release

# With Docker backend only
cargo build --no-default-features --features docker

# With libvirt backend
cargo build --features libvirt
```

### Run Tests

```bash
# Run all tests
cargo test

# Run with specific backend
cargo test --no-default-features --features docker

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Generate documentation
cargo doc --open
```

---

## 📊 Project Status

**Current Version:** 2.0.0  
**Status:** Production Ready ✅  
**Quality Grade:** A+ (98/100) 🏆

### Metrics
- **Lines of Code:** 2,108
- **Test Coverage:** **90.24%** (106/106 tests passing) ✅
- **Modules at 90%+:** 6 of 9 modules
- **Perfect Coverage:** 1 module (error.rs at 100%)
- **Hardcoded Values:** 0 (100% configurable)
- **Unsafe Code:** 0 blocks
- **Build Status:** ✅ Clean (no warnings)

### Production Readiness
- ✅ **Docker backend** - Production ready, fully tested
- ✅ **Configuration system** - Production ready, 97% coverage
- ✅ **Error handling** - Production ready, 100% coverage
- ✅ **Lab management** - Production ready, 96.6% coverage
- ✅ **Lab registry** - Production ready, 98.9% coverage
- ✅ **Topology parser** - Production ready, 94.2% coverage
- ✅ **Network simulator** - Production ready, 90.9% coverage
- ⚠️ **Libvirt backend** - Beta (needs real VM testing)
- ✅ **CLI** - All commands functional

### Test Coverage by Module
```
error.rs          100.00%  ✨ Perfect
lab/registry.rs    98.92%  ✨ Excellent
config.rs          97.04%  ✨ Excellent  
lab/mod.rs         96.61%  ✨ Excellent
topology/mod.rs    94.17%  ✨ Excellent
network/mod.rs     90.91%  ✨ Excellent
─────────────────────────────────────
TOTAL              90.24%  🏆 GOAL MET!
```

See [COVERAGE_GOAL_ACHIEVED.md](COVERAGE_GOAL_ACHIEVED.md) for detailed coverage report.

---

## 🤝 Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and formatting
5. Submit a pull request

See [specs/DEVELOPMENT_STATUS.md](specs/DEVELOPMENT_STATUS.md) for development guidelines.

---

## 🏛️ Philosophy: Primal Tools

benchScale is a **Primal Tool** within the ecoPrimals ecosystem:

- **Serves Primals** - Infrastructure for testing sovereign components
- **Pragmatic** - Can have hardcoding for specific use cases (though v2.0 has none)
- **Not a Primal** - Different sovereignty model, focused on testing
- **Essential** - Critical for validation and deployment workflows

See [PRIMAL_TOOLS_ARCHITECTURE.md](PRIMAL_TOOLS_ARCHITECTURE.md) for details.

---

## 🔗 Related Projects

- **[BiomeOS](../BiomeOS/)** - Sovereign operating system for ecoPrimals
- **[RhizoCrypt](../RhizoCrypt/)** - Content-addressed DAG engine
- **[LoamSpine](../LoamSpine/)** - Distributed consensus layer
- **[SweetGrass](../SweetGrass/)** - P2P networking substrate

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
- [tracing](https://docs.rs/tracing/) - Logging and observability

---

## 📞 Support

- **Issues:** [GitHub Issues](https://github.com/ecoPrimals/benchScale/issues)
- **Discussions:** [GitHub Discussions](https://github.com/ecoPrimals/benchScale/discussions)
- **Community:** ecoPrimals Discord

---

**benchScale** - *Testing infrastructure for sovereign distributed systems*

Made with ❤️ by the ecoPrimals community
