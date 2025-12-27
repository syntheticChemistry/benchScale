# benchScale Technical Specification

**Version:** 2.0.0  
**Date:** December 27, 2025  
**Status:** Beta Quality - Production Ready  
**Quality Grade:** B (80/100)  
**Authors:** benchScale Team

---

## 1. Executive Summary

### 1.1 Purpose

benchScale is a **pure Rust laboratory substrate** for creating reproducible, isolated test environments for distributed systems, P2P networks, and multi-node applications. It serves as a Primal Tool within the ecoPrimals ecosystem, enabling developers and operators to test complex distributed scenarios before production deployment.

### 1.2 Key Features

- **Declarative Topologies** - Define networks in YAML
- **Multiple Backends** - Docker (production), libvirt/KVM (complete)
- **Network Simulation** - Latency, packet loss, bandwidth constraints
- **Test Orchestration** - Automated test scenarios with validation
- **BiomeOS Integration** - First-class support for BiomeOS VMs
- **Type-Safe API** - Pure Rust with comprehensive error handling
- **Zero Hardcoding** - Environment-driven configuration (15+ options)
- **Lab Persistence** - Registry for managing lab state across sessions
- **Health Monitoring** - VM boot detection and network readiness checks
- **Serial Console** - BootLogger parsing and log analysis

### 1.3 Non-Goals

- **Not a production orchestrator** - Use Kubernetes/Nomad for that
- **Not a CI/CD system** - Integrates with CI/CD, doesn't replace it
- **Not a monitoring solution** - Provides observability hooks only
- **Not a Primal** - It's a Primal Tool (different sovereignty model)

---

## 2. Architecture

### 2.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application Layer                     │
│  (BiomeOS, User Apps, CI/CD Scripts, Test Runners)     │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│                  benchScale Core (Rust)                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Topology   │  │  Lab Manager │  │ Test Runner  │  │
│  │   Parser     │  │              │  │              │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │            Backend Trait (Abstraction)           │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
         ┌───────────────┴───────────────┐
         ▼                               ▼
┌──────────────────┐            ┌──────────────────┐
│  DockerBackend   │            │ LibvirtBackend   │
│  (bollard)       │            │ (virt-rs)        │
└────────┬─────────┘            └────────┬─────────┘
         │                               │
         ▼                               ▼
┌──────────────────┐            ┌──────────────────┐
│  Docker Daemon   │            │  libvirt/QEMU    │
│  (Containers)    │            │  (VMs)           │
└──────────────────┘            └──────────────────┘
```

### 2.2 Core Components

#### 2.2.1 Topology Parser (`src/topology/`)

**Purpose:** Parse and validate YAML topology definitions

**Responsibilities:**
- Load topology from YAML files
- Validate configuration (subnet, node names, conditions)
- Provide type-safe access to topology data
- Merge node-specific and network-level settings

**Key Types:**
```rust
pub struct Topology {
    pub metadata: TopologyMetadata,
    pub network: NetworkConfig,
    pub nodes: Vec<NodeConfig>,
}

pub struct NodeConfig {
    pub name: String,
    pub image: String,
    pub env: HashMap<String, String>,
    pub network_conditions: Option<NetworkConditions>,
    // ...
}
```

#### 2.2.2 Lab Manager (`src/lab/`)

**Purpose:** Orchestrate lab lifecycle and state management

**Responsibilities:**
- Create labs from topologies
- Manage lab lifecycle (create, run, destroy)
- Track lab state (nodes, networks, status)
- Coordinate backend operations
- Deploy applications to nodes
- Execute commands on nodes

**Key Types:**
```rust
pub struct Lab {
    id: String,
    name: String,
    topology: Topology,
    backend: Arc<dyn Backend>,
    state: Arc<RwLock<LabState>>,
}

pub enum LabStatus {
    Creating,
    Running,
    Destroying,
    Destroyed,
    Failed,
}
```

#### 2.2.3 Backend Trait (`src/backend/mod.rs`)

**Purpose:** Abstract over container and VM runtimes

**Contract:**
```rust
#[async_trait]
pub trait Backend: Send + Sync {
    // Network management
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo>;
    async fn delete_network(&self, name: &str) -> Result<()>;
    
    // Node management
    async fn create_node(&self, name: &str, image: &str, network: &str, env: HashMap<String, String>) -> Result<NodeInfo>;
    async fn start_node(&self, node_id: &str) -> Result<()>;
    async fn stop_node(&self, node_id: &str) -> Result<()>;
    async fn delete_node(&self, node_id: &str) -> Result<()>;
    async fn get_node(&self, node_id: &str) -> Result<NodeInfo>;
    
    // Operations
    async fn exec_command(&self, node_id: &str, command: Vec<String>) -> Result<ExecResult>;
    async fn copy_to_node(&self, node_id: &str, src: &str, dest: &str) -> Result<()>;
    async fn get_logs(&self, node_id: &str) -> Result<String>;
    
    // Network simulation
    async fn apply_network_conditions(&self, node_id: &str, latency_ms: Option<u32>, packet_loss_percent: Option<f32>, bandwidth_kbps: Option<u32>) -> Result<()>;
    
    // Health
    async fn is_available(&self) -> Result<bool>;
}
```

#### 2.2.4 Docker Backend (`src/backend/docker.rs`)

**Implementation:** Uses `bollard` crate for Docker API

**Features:**
- Container lifecycle management
- Bridge network creation
- Traffic control (tc) for network simulation
- Image pulling (standard and hardened)
- File transfer via tar archives
- Log streaming

**Status:** ✅ Fully implemented (470 lines)

#### 2.2.5 Libvirt Backend (`src/backend/libvirt.rs`)

**Implementation:** Uses `virt` crate for libvirt API

**Features:**
- VM lifecycle management (complete)
- qcow2 disk overlay management (copy-on-write)
- Network bridge creation
- SSH-based command execution
- File transfer via SCP
- Serial console capture and parsing
- VM health monitoring
- IP discovery with timeout

**Status:** ✅ Fully implemented (548 lines)
- ✅ Network management
- ✅ VM creation with disk overlays
- ✅ VM start/stop/destroy
- ✅ Log retrieval (serial console)
- ✅ Serial console integration
- ✅ Health monitoring
- ✅ Automatic cleanup

#### 2.2.6 Configuration System (`src/config.rs`)

**Purpose:** Environment-driven configuration for zero hardcoding

**Features:**
- 15+ configurable options via environment variables
- TOML configuration file support
- Type-safe configuration structs
- Sensible defaults with fallbacks
- Duration helpers for timeouts

**Configuration Options:**
```bash
# Libvirt
BENCHSCALE_LIBVIRT_URI=qemu:///system
BENCHSCALE_BASE_IMAGE_PATH=/var/lib/libvirt/images
BENCHSCALE_OVERLAY_DIR=/tmp/benchscale

# SSH
BENCHSCALE_SSH_USER=myuser
BENCHSCALE_SSH_PASSWORD=mypass
BENCHSCALE_SSH_KEY=~/.ssh/id_rsa
BENCHSCALE_SSH_PORT=22
BENCHSCALE_SSH_TIMEOUT_SECS=30

# Docker
BENCHSCALE_HARDENED_IMAGES=true
BENCHSCALE_DOCKER_TIMEOUT_SECS=60

# Lab
BENCHSCALE_STATE_DIR=/var/lib/benchscale
BENCHSCALE_DEFAULT_NETWORK_BRIDGE=br0
```

**Status:** ✅ Fully implemented (332 lines)

#### 2.2.7 Lab Registry (`src/lab/registry.rs`)

**Purpose:** Persistent lab state management across CLI sessions

**Features:**
- JSON-based metadata storage
- Register/load/delete operations
- List all labs with sorting
- Stale lab cleanup by age
- Full CRUD operations

**Key Types:**
```rust
pub struct LabMetadata {
    pub id: String,
    pub name: String,
    pub status: LabStatus,
    pub topology: Topology,
    pub backend_type: String,
    pub node_ids: Vec<String>,
    pub network_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Status:** ✅ Fully implemented (310 lines)

#### 2.2.8 VM Utilities (`src/backend/vm_utils.rs`)

**Purpose:** Disk image management and libvirt XML generation

**Features:**
- qcow2 disk overlay creation (copy-on-write)
- Libvirt domain XML generation
- Memory parsing (2G, 512M, etc.)
- Automatic overlay cleanup
- qemu-img availability check

**Status:** ✅ Fully implemented (185 lines)

#### 2.2.9 Serial Console (`src/backend/serial_console.rs`)

**Purpose:** Parse and analyze VM serial console output

**Features:**
- BiomeOS BootLogger detection
- Boot completion checking
- Boot time extraction (milliseconds)
- Log statistics (info/warn/error counts)
- Error message extraction

**Status:** ✅ Fully implemented (119 lines)

#### 2.2.10 Health Monitoring (`src/backend/health.rs`)

**Purpose:** VM health status checking and monitoring

**Features:**
- Health status enum (Healthy/Booting/Unhealthy/Unknown)
- Boot completion tracking
- Network reachability checks
- Error log analysis
- Wait-for-healthy helper

**Key Types:**
```rust
pub struct HealthCheck {
    pub status: HealthStatus,
    pub boot_complete: bool,
    pub boot_time_ms: Option<u64>,
    pub network_reachable: bool,
    pub check_duration: Duration,
    pub errors: Vec<String>,
}
```

**Status:** ✅ Fully implemented (250 lines)

#### 2.2.11 Network Simulator (`src/network/`)

**Purpose:** Apply network conditions to nodes

**Features:**
- Preset conditions (LAN, WAN, cellular, slow)
- Custom conditions (latency, loss, bandwidth)
- Delegation to backend for application

**Implementation:**
```rust
pub struct NetworkSimulator;

impl NetworkSimulator {
    pub async fn apply_conditions(&self, backend: Arc<dyn Backend>, node_id: &str, conditions: &NetworkConditions) -> Result<()>;
    
    // Presets
    pub fn lan_conditions() -> NetworkConditions;
    pub fn wan_conditions() -> NetworkConditions;
    pub fn cellular_conditions() -> NetworkConditions;
}
```

#### 2.2.7 Test Runner (`src/tests/`)

**Purpose:** Execute test scenarios and collect results

**Features:**
- Load test scenarios from YAML
- Execute steps sequentially
- Validate exit codes
- Collect stdout/stderr
- Report results with timing

**Types:**
```rust
pub struct TestScenario {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<TestStep>,
    pub timeout: Option<Duration>,
}

pub struct TestResult {
    pub scenario: String,
    pub success: bool,
    pub steps: Vec<StepResult>,
    pub duration: Duration,
    pub error: Option<String>,
}
```

---

## 3. Data Models

### 3.1 Topology Format (YAML)

```yaml
metadata:
  name: string                    # Topology name (required)
  description: string             # Human-readable description (optional)
  version: string                 # Topology version (optional)
  tags: [string]                  # Tags for categorization (optional)

network:
  name: string                    # Network name (required)
  subnet: string                  # CIDR notation (required, e.g., "10.100.0.0/24")
  conditions:                     # Network-level conditions (optional)
    latency_ms: u32               # Latency in milliseconds
    packet_loss_percent: f32      # Packet loss 0-100%
    bandwidth_kbps: u32           # Bandwidth in kbps

nodes:
  - name: string                  # Node name (required, unique)
    image: string                 # Docker image or VM disk image (required)
    env:                          # Environment variables (optional)
      KEY: VALUE
    ports:                        # Port mappings (optional, Docker only)
      - "host:container"
    volumes:                      # Volume mounts (optional)
      - "host:container"
    network_conditions:           # Node-specific conditions (optional, overrides network-level)
      latency_ms: u32
      packet_loss_percent: f32
      bandwidth_kbps: u32
    metadata:                     # Custom metadata (optional)
      key: value
```

### 3.2 Test Scenario Format (YAML)

```yaml
- name: string                    # Scenario name (required)
  description: string             # Description (optional)
  timeout: duration               # Overall timeout (optional)
  steps:
    - name: string                # Step name (required)
      node: string                # Target node name (required)
      command: [string]           # Command to execute (required)
      expected_exit_code: i64     # Expected exit code (default: 0)
      timeout: duration           # Step timeout (optional)
```

### 3.3 Internal State Models

```rust
// Lab state (internal)
struct LabState {
    status: LabStatus,
    network_id: Option<String>,
    nodes: HashMap<String, NodeInfo>,
    error: Option<String>,
}

// Node information (backend-agnostic)
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub container_id: String,      // Or VM ID
    pub ip_address: String,
    pub network: String,
    pub status: NodeStatus,
    pub metadata: HashMap<String, String>,
}

// Network information
pub struct NetworkInfo {
    pub name: String,
    pub id: String,
    pub subnet: String,
    pub gateway: String,
}
```

---

## 4. API Contracts

### 4.1 Rust API

#### Creating a Lab

```rust
use benchscale::{Lab, DockerBackend, Topology};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load topology
    let topology = Topology::from_file("topologies/simple-lan.yaml").await?;
    
    // Create backend
    let backend = DockerBackend::new()?;
    
    // Create lab
    let lab = Lab::create("my-lab", topology, backend).await?;
    
    // Lab is now running
    assert_eq!(lab.status().await, LabStatus::Running);
    
    // Cleanup
    lab.destroy().await?;
    
    Ok(())
}
```

#### Running Tests

```rust
use benchscale::{TestScenario, TestStep};

let scenario = TestScenario {
    name: "connectivity-test".to_string(),
    description: Some("Verify nodes can communicate".to_string()),
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

let results = lab.run_tests(vec![scenario]).await?;
assert!(results[0].success);
```

### 4.2 CLI API

```bash
# Create lab from topology
benchscale create <lab-name> <topology-file>

# List active labs
benchscale list

# Get lab status
benchscale status <lab-name>

# Get logs from node
benchscale logs <lab-name> <node-name>

# Destroy lab
benchscale destroy <lab-name>

# Show version
benchscale version
```

---

## 5. Network Simulation

### 5.1 Traffic Control (tc)

Network conditions are applied using Linux `tc` (traffic control):

```bash
# Applied inside containers/VMs
tc qdisc add dev eth0 root netem \
    delay 50ms \
    loss 0.1% \
    rate 100mbit
```

### 5.2 Supported Conditions

| Parameter | Type | Range | Unit | Notes |
|-----------|------|-------|------|-------|
| `latency_ms` | u32 | 0-10000 | milliseconds | One-way delay |
| `packet_loss_percent` | f32 | 0.0-100.0 | percent | Random packet drop |
| `bandwidth_kbps` | u32 | 1-∞ | kilobits/sec | Rate limiting |

### 5.3 Preset Conditions

```rust
// LAN: Low latency, no loss, high bandwidth
NetworkSimulator::lan_conditions()
// => latency: 1ms, loss: 0%, bandwidth: 1Gbps

// WAN: Medium latency, minimal loss
NetworkSimulator::wan_conditions()
// => latency: 50ms, loss: 0.1%, bandwidth: 100Mbps

// Cellular: Higher latency, some loss
NetworkSimulator::cellular_conditions()
// => latency: 100ms, loss: 2%, bandwidth: 50Mbps

// Slow: High latency, significant loss
NetworkSimulator::slow_network_conditions()
// => latency: 200ms, loss: 5%, bandwidth: 10Mbps
```

---

## 6. Error Handling

### 6.1 Error Types

```rust
pub enum Error {
    Docker(bollard::errors::Error),
    Backend(String),
    Topology(String),
    Network(String),
    Lab(String),
    Test(String),
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Json(serde_json::Error),
    Other(String),
}
```

### 6.2 Error Handling Strategy

- **Propagation:** Use `?` operator throughout async code
- **Context:** Wrap errors with context using `anyhow` at application boundaries
- **Recovery:** Attempt cleanup on failure (best effort)
- **Logging:** Use `tracing` for structured error logging

---

## 7. Integration Patterns

### 7.1 BiomeOS Integration

benchScale integrates with BiomeOS for lab orchestration:

```rust
// BiomeOS can create labs programmatically
use biomeos_core::lab::LabManager;

let lab_manager = LabManager::new();
let lab = lab_manager.create_lab("simple-lan", "my-lab").await?;
lab.deploy("templates/p2p-secure-mesh.biome.yaml").await?;
let result = lab.run_test("btsp-tunnels").await?;
lab.destroy().await?;
```

See `BIOMEOS_INTEGRATION.md` for details.

### 7.2 CI/CD Integration

```yaml
# GitHub Actions example
- name: Create test lab
  run: benchscale create test-lab topologies/simple-lan.yaml

- name: Deploy application
  run: benchscale exec test-lab node-1 /deploy.sh

- name: Run integration tests
  run: benchscale test test-lab scenarios/integration.yaml

- name: Cleanup
  if: always()
  run: benchscale destroy test-lab
```

---

## 8. Security Considerations

### 8.1 Container/VM Isolation

- **Network Isolation:** Each lab gets its own network
- **Resource Limits:** Honor Docker/libvirt resource constraints
- **Capability Restrictions:** Minimal capabilities (except NET_ADMIN for tc)

### 8.2 Credentials

- **SSH Keys:** Prefer SSH keys over passwords
- **Environment Variables:** Never log sensitive env vars
- **Cleanup:** Destroy labs on error to prevent resource leaks

### 8.3 Image Security

- **Hardened Images:** Support for Docker hardened images
- **Image Verification:** Future: verify image signatures
- **Minimal Base Images:** Encourage Alpine/distroless

---

## 9. Performance Characteristics

### 9.1 Expected Performance

| Operation | Docker Backend | Libvirt Backend | Notes |
|-----------|----------------|-----------------|-------|
| Create 1-node lab | 3-5s | 10-15s | VM boot slower |
| Create 3-node lab | 5-10s | 20-30s | Parallel creation |
| Destroy lab | 1-2s | 3-5s | Force cleanup |
| Execute command | 100-200ms | 200-500ms | SSH overhead |
| Copy file (1MB) | 200-300ms | 500-1000ms | Tar vs SCP |

### 9.2 Resource Usage

| Resource | Docker (per node) | VM (per node) | Notes |
|----------|-------------------|---------------|-------|
| Memory | 64MB-2GB | 512MB-4GB | Configurable |
| Disk | 100MB-1GB | 2GB-10GB | Overlay layers |
| CPU | Shared | 1-4 vCPUs | Configurable |

---

## 10. Future Enhancements

### 10.1 Planned Features (v2.1)

- ✅ BiomeOS VM support (libvirt backend completion)
- ✅ Serial console capture for VMs
- ✅ Disk image management (qcow2 overlays)
- ✅ VM health monitoring

### 10.2 Future Considerations (v2.2+)

- **Kubernetes Backend:** Deploy to K8s clusters
- **Cloud Backends:** AWS, GCP, Azure
- **GUI/TUI:** Terminal UI for lab management
- **Metrics Export:** Prometheus/Grafana integration
- **Snapshot/Restore:** Lab state snapshots
- **Multi-Host:** Distributed labs across machines

---

## 11. References

### 11.1 External References

- **Docker API:** https://docs.docker.com/engine/api/
- **libvirt:** https://libvirt.org/
- **tc (traffic control):** https://man7.org/linux/man-pages/man8/tc.8.html
- **QEMU:** https://www.qemu.org/

### 11.2 Internal References

- **Phase 2 Architecture:** `../../ARCHITECTURE.md`
- **Primal Tools:** `../PRIMAL_TOOLS_ARCHITECTURE.md`
- **BiomeOS Integration:** `../BIOMEOS_INTEGRATION.md`
- **Development Status:** `./DEVELOPMENT_STATUS.md`

---

**Specification Version:** 2.0.0  
**Last Updated:** December 27, 2025  
**Status:** Active Development  
**Next Review:** January 24, 2026

