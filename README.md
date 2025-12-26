# benchScale v2.0.0

**A Pure Rust Laboratory Substrate for Distributed System Testing**

benchScale provides a type-safe, declarative framework for creating reproducible test environments for distributed systems, P2P networks, and multi-node applications. Built on Docker with first-class support for hardened images.

---

## 🎯 Purpose

benchScale elevates from an ecoPrimals development tool to a **universal laboratory substrate**, enabling:

- **Reproducible Testing**: Declarative YAML topologies ensure consistent environments
- **Network Simulation**: Real-world conditions (latency, packet loss, bandwidth, NAT)
- **Security-First**: Native support for Docker hardened images
- **Pure Rust**: No shell scripts, full type safety, better error handling
- **Cross-Platform**: Works on Linux, macOS, and Windows with Docker

---

## 🚀 Quick Start

### Prerequisites

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh

# Verify Docker is running
docker ps
```

### Installation

```bash
# Clone repository
git clone git@github.com:ecoPrimals/benchScale.git
cd benchScale

# Build
cargo build --release

# Run
./target/release/benchscale --version
```

### Your First Lab

Create a simple 2-node LAN topology:

```bash
# Create lab from topology
./target/release/benchscale create my-lab topologies/simple-lan.yaml

# Verify nodes are running
docker ps | grep my-lab

# Destroy lab
./target/release/benchscale destroy my-lab
```

---

## 📖 Features

### Pure Rust Architecture

- **No Shell Scripts**: Direct Docker API integration via `bollard`
- **Type-Safe**: Comprehensive type system for lab management
- **Async/Await**: Modern async Rust throughout
- **Error Handling**: Rich error types with context

### Network Simulation

- **Latency Injection**: Simulate WAN/LAN latencies
- **Packet Loss**: Configure realistic packet drop rates
- **Bandwidth Limiting**: Constrain network throughput
- **NAT Simulation**: Test P2P traversal scenarios

### Docker Hardened Images 🔒

```rust
use benchscale::{Lab, DockerBackend, Topology};

// Use hardened images automatically
let backend = DockerBackend::new_hardened()?;
let lab = Lab::create("secure-lab", topology, backend).await?;
```

Hardened images provide:
- Minimal attack surface
- Regular security updates
- Best practices baked in
- Official Docker support

### Declarative Topologies

Define complex network topologies in YAML:

```yaml
metadata:
  name: p2p-3-tower
  description: "3-node P2P federation"
  version: "2.0"
  tags: ["p2p", "federation", "multi-region"]

network:
  name: p2p-federation
  subnet: "10.200.0.0/24"

nodes:
  - name: tower-sf
    image: ubuntu
    env:
      REGION: san-francisco
    network_conditions:
      latency_ms: 5
      packet_loss_percent: 0.1
      bandwidth_kbps: 100000

  - name: tower-ny
    image: ubuntu
    env:
      REGION: new-york
    network_conditions:
      latency_ms: 50
      packet_loss_percent: 0.2
      bandwidth_kbps: 100000

  - name: tower-london
    image: ubuntu
    env:
      REGION: london
    network_conditions:
      latency_ms: 100
      packet_loss_percent: 0.5
      bandwidth_kbps: 50000
```

---

## 🦀 Rust API

### Basic Usage

```rust
use benchscale::{Lab, DockerBackend, Topology};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load topology
    let topology = Topology::from_file("topologies/simple-lan.yaml").await?;
    
    // Create Docker backend
    let backend = DockerBackend::new()?;
    
    // Create lab
    let lab = Lab::create("my-lab", topology, backend).await?;
    
    // Deploy binary to a node
    lab.deploy_to_node("node-1", "/path/to/binary").await?;
    
    // Execute command
    let result = lab.exec_on_node(
        "node-1",
        vec!["./binary".to_string(), "arg1".to_string()],
    ).await?;
    
    println!("Exit code: {}", result.exit_code);
    println!("Output: {}", result.stdout);
    
    // Run test scenarios
    let scenarios = vec![/* ... */];
    let results = lab.run_tests(scenarios).await?;
    
    // Cleanup
    lab.destroy().await?;
    
    Ok(())
}
```

### Test Scenarios

```rust
use benchscale::TestScenario;

let scenario = TestScenario {
    name: "connectivity-test".to_string(),
    description: Some("Verify network connectivity".to_string()),
    steps: vec![
        TestStep {
            name: "ping-node-2".to_string(),
            node: "node-1".to_string(),
            command: vec!["ping".to_string(), "-c".to_string(), "3".to_string(), "node-2".to_string()],
            expected_exit_code: 0,
            timeout: Some(Duration::from_secs(10)),
        },
    ],
    timeout: Some(Duration::from_secs(30)),
};
```

---

## 📊 Architecture

```
BiomeOS (or any Rust application)
└── benchscale (Rust crate)
    └── bollard (Docker API client)
        └── Docker daemon
            └── Containers (standard or hardened)
```

### Core Components

- **`backend/`**: Docker backend implementation using bollard
- **`topology/`**: YAML topology parsing and validation
- **`lab/`**: High-level lab management API
- **`network/`**: Network simulation (tc - traffic control)
- **`tests/`**: Test scenario runner

---

## 🔧 CLI Commands

```bash
# Create a lab
benchscale create <name> <topology-file>

# Destroy a lab
benchscale destroy <name>

# List active labs
benchscale list

# Show version
benchscale version
```

---

## 📐 Topology Examples

### Simple LAN (2 nodes)
- **File**: `topologies/simple-lan.yaml`
- **Use Case**: Basic connectivity testing
- **Network**: Low latency, high bandwidth

### P2P 3-Tower Federation
- **File**: `topologies/p2p-3-tower.yaml`
- **Use Case**: Multi-region P2P testing
- **Network**: Varying latencies (SF, NY, London)

### NAT Traversal (4 nodes)
- **File**: `topologies/nat-traversal.yaml`
- **Use Case**: P2P relay and NAT hole-punching
- **Network**: Relay server + 3 clients behind NAT

---

## 🔐 Security

### Hardened Images

Enable hardened image support:

```toml
# Cargo.toml
[dependencies]
benchscale = { version = "2.0", features = ["hardened"] }
```

```rust
// Use hardened backend
let backend = DockerBackend::new_hardened()?;
```

Supported hardened images:
- `ubuntu` → `docker.io/dockerhardened/ubuntu:latest`
- `alpine` → `docker.io/dockerhardened/alpine:latest`
- `debian` → `docker.io/dockerhardened/debian:latest`

---

## 🛠️ Development

### Build from Source

```bash
git clone git@github.com:ecoPrimals/benchScale.git
cd benchScale
cargo build --release
cargo test
```

### Run Tests

```bash
# Unit tests
cargo test --lib

# Integration tests (requires Docker)
cargo test --test '*'
```

---

## 📚 Documentation

- **[API Documentation](https://docs.rs/benchscale)**: Full Rust API docs
- **[BIOMEOS_INTEGRATION.md](BIOMEOS_INTEGRATION.md)**: BiomeOS integration guide
- **[PRIMAL_TOOLS_ARCHITECTURE.md](PRIMAL_TOOLS_ARCHITECTURE.md)**: Architecture philosophy
- **[QUICKSTART.md](QUICKSTART.md)**: Detailed getting started guide

---

## 🤝 Contributing

benchScale is part of the ecoPrimals ecosystem. Contributions are welcome!

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

---

## 📄 License

MIT OR Apache-2.0

---

## 🌟 From ecoPrimals Tool to Laboratory Substrate

benchScale v2.0 represents a significant evolution:

**v1.0** (Shell Scripts + LXD):
- ❌ Shell script dependencies
- ❌ Ubuntu/LXD-specific
- ❌ Manual process management
- ❌ Limited error handling

**v2.0** (Pure Rust + Docker):
- ✅ Pure Rust implementation
- ✅ Cross-platform (Docker everywhere)
- ✅ Type-safe API
- ✅ Hardened image support
- ✅ Production-ready

This transformation elevates benchScale from an ecoPrimals development tool to a **universal laboratory substrate** for distributed system testing.

---

**Built with 🦀 Rust | Powered by 🐳 Docker | Secured by 🔒 Hardened Images**

For support: dev@ecoprimals.org  
Repository: https://github.com/ecoPrimals/benchScale
