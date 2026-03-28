# benchScale - VM Orchestration & Substrate Provisioning

**Production-ready Rust framework for distributed system testing**

🟢 **Status**: Production Ready — v3.0.0 (Evolutions #20-#23 Complete)  
📅 **Last Updated**: March 28, 2026  
🧪 **Tests**: 212/212 passing (100%)  
🔒 **Safety**: Zero unsafe code

---

## Quick Start

```bash
# Build benchScale
cd benchScale
cargo build --release

# Run tests
cargo test

# Create a VM with cloud-init
cargo run --example production_vm_ready
```

---

## What is benchScale?

**benchScale** is a pure Rust VM orchestration framework that provides:

- **Type-Safe Configuration**: Comprehensive, validated configuration system
- **Self-Healing Infrastructure**: Automatic health checks and recovery (Evolution #20)
- **DHCP Discovery**: MAC-based VM tracking with lease renewal (Evolution #22)
- **Cloud-Init Integration**: Native support for cloud-init provisioning
- **Real-Time Monitoring**: VM senescence tracking with configurable thresholds (Evolution #21)
- **Zero Hardcoding**: Capability-based design with runtime discovery

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│         Application Layer (agentReagents, CI/CD)        │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│              benchScale Core (Rust)                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Configuration│  │ Senescence   │  │ Health Check │  │
│  │ System       │  │ Monitor      │  │ & Recovery   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Backend Trait (Abstraction)              │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────┐
│         LibvirtBackend (Production Ready)               │
│  • VM lifecycle • DHCP discovery • Health monitoring    │
│  • Cloud-init • Boot diagnostics • VmGuard cleanup      │
└─────────────────────────────────────────────────────────┘
```

---

## Features

### 🎯 Configuration System (Phase 2 & 3A)

Type-safe, validated, YAML-serializable configuration with runtime discovery:

```rust
use benchscale::config::{BenchScaleConfig, MonitoringConfig};

// Use workload-specific presets
let config = BenchScaleConfig {
    monitoring: MonitoringConfig::for_cloud_init_packages(), // 30min tolerance
    ..Default::default()
};

// Configuration-driven monitoring
let monitor = SenescenceMonitor::from_config(
    vm_name, ip, mac_address, &config.monitoring
);
```

**Modules**:
- `TimeoutConfig` - All timeout settings
- `MonitoringConfig` - Health check parameters with workload presets
- `NetworkConfig` - Network discovery, DHCP, SSH (Phase 2C)
- `StorageConfig` - Storage paths, limits, COW settings (Phase 2C)
- `VirtConfig` - Virtualization settings (Phase 3A)

**Features**:
- ✅ 77 comprehensive tests
- ✅ Sensible defaults (zero-config operation)
- ✅ Load-time validation
- ✅ Workload-specific presets
- ✅ Runtime discovery integration

### 🏥 Self-Healing Infrastructure (Evolution #20)

Automatic health checks and recovery for libvirt:

```rust
// Backend automatically ensures health before operations
let backend = LibvirtBackend::new()?;
backend.ensure_healthy().await?; // Self-heals if needed
```

**Features**:
- Libvirtd service status checking
- Orphaned process detection (daemonization-aware)
- Network state verification
- DHCP functionality validation
- Sudo-free recovery via `virsh` API
- Graceful degradation on partial failures

### 📊 VM Senescence Monitoring (Evolution #21)

Real-time health tracking with configurable thresholds:

```rust
let monitor = SenescenceMonitor::from_config(
    vm_name,
    ip_address,
    mac_address,
    &MonitoringConfig::for_cloud_init_packages() // 30min tolerance
);

// Monitor automatically tracks:
// - Ping availability
// - SSH connectivity
// - Cloud-init completion
// - DHCP lease changes
// - Stall detection
```

**Workload Presets**:
- Quick VMs: 10 failures (100s tolerance)
- Desktop: 60 failures (10min tolerance)
- Cloud-init packages: 180 failures (30min tolerance)

### 🌐 DHCP Lease Tracking (Evolution #22)

Handles IP address changes during long-running builds:

```rust
// Monitor automatically re-discovers IP via MAC address
let monitor = SenescenceMonitor::with_mac_address(
    vm_name,
    initial_ip,
    mac_address  // Used for IP re-discovery
);

// Periodic re-discovery (every 100 checks)
// - Query libvirt DHCP leases by MAC
// - Update internal IP reference
// - Continue monitoring seamlessly
```

**Features**:
- MAC-based VM identification
- Periodic IP re-discovery (configurable interval)
- Transparent to consumers
- Prevents false negatives from IP changes

### 🔍 Robust Package Verification (Evolution #23)

Multi-method verification with rich diagnostics:

```rust
// Verification automatically uses multiple methods:
// 1. dpkg-query (most reliable)
// 2. dpkg -l (standard)
// 3. apt-cache policy (repository check)
// 4. Dependency check (transitively installed)
```

**Features**:
- Architecture suffix handling (`:amd64`)
- Wildcard fallback for partial matches
- Rich diagnostics for troubleshooting
- False negative detection

### 🦀 Pure Rust Implementation

- **Zero unsafe code** (enforced with `#![deny(unsafe_code)]`)
- **Zero production mocks** (all isolated to tests)
- **Modern async/await** throughout
- **Type-safe APIs** with compile-time guarantees
- **212 tests passing** (100%)

### 🧬 Primal Philosophy

- **Self-Knowledge**: Components discover capabilities at runtime
- **Runtime Discovery**: SystemCapabilities for paths, networks, storage
- **Capability-Based**: No hardcoded assumptions about environment
- **Fractal/Isomorphic**: Patterns consistent across all scales
- **Zero-Cost Abstractions**: Fast AND safe

---

## Core Components

### Backend Trait

Abstraction over VM/container runtimes:

```rust
#[async_trait]
pub trait Backend {
    async fn create_node(&self, ...) -> Result<NodeInfo>;
    async fn delete_node(&self, name: &str) -> Result<()>;
    async fn list_nodes(&self) -> Result<Vec<NodeInfo>>;
    async fn get_node_status(&self, name: &str) -> Result<NodeStatus>;
    async fn ensure_healthy(&self) -> Result<()>; // Evolution #20
}
```

### LibvirtBackend

Production-ready libvirt/KVM backend:

**Features**:
- VM lifecycle management (create, delete, list, status)
- Cloud-init integration with validation
- DHCP discovery via MAC addresses
- SSH execution and service validation
- Health checks and auto-recovery
- Boot diagnostics (serial console, systemd)
- VmGuard for automatic cleanup (RAII pattern)

**Evolutions**:
- ✅ Evolution #20: Health check & recovery
- ✅ Evolution #21: Configurable monitoring
- ✅ Evolution #22: DHCP lease tracking
- ✅ Evolution #23: Robust verification

### SenescenceMonitor

Real-time VM health tracking:

```rust
pub struct SenescenceMonitor {
    metrics: Arc<RwLock<SenescenceMetrics>>,
    start_time: Instant,
    check_interval: Duration,
    stall_threshold: Duration,
    max_failures: usize,
    ip_rediscovery_interval: usize,
}
```

**Tracks**:
- Ping availability
- SSH connectivity
- Cloud-init completion
- Uptime and health duration
- Consecutive failures
- IP address changes (Evolution #22)

### SystemCapabilities

Runtime environment discovery:

```rust
pub struct SystemCapabilities {
    pub network: NetworkCapabilities,
    pub storage: StorageCapabilities,
    pub virtualization: VirtCapabilities,
}
```

**Discovers**:
- Libvirt URI and networks
- VM image directories
- Cloud-init paths
- Network interfaces
- DHCP ranges
- OS variants

---

## Configuration

### Environment Variables

```bash
# Monitoring settings
export BENCHSCALE_MONITORING_CHECK_INTERVAL_SECS=10
export BENCHSCALE_MONITORING_STALL_THRESHOLD_SECS=60
export BENCHSCALE_MONITORING_MAX_FAILURES=180
export BENCHSCALE_MONITORING_IP_REDISCOVERY_INTERVAL=100

# Timeout settings
export BENCHSCALE_CLOUD_INIT_TIMEOUT_SECS=1800
export BENCHSCALE_SSH_TIMEOUT_SECS=300
export BENCHSCALE_BOOT_TIMEOUT_SECS=300

# Network settings
export BENCHSCALE_NETWORK_NAME="default"
export BENCHSCALE_DHCP_RANGE_START="192.168.122.2"
export BENCHSCALE_DHCP_RANGE_END="192.168.122.254"
export BENCHSCALE_DHCP_DISCOVERY_TIMEOUT_SECS=30
export BENCHSCALE_SSH_PORT=22

# Storage settings
export BENCHSCALE_VM_IMAGES_DIR="/var/lib/libvirt/images"
export BENCHSCALE_BASE_IMAGES_DIR="/var/lib/libvirt/images/base"
export BENCHSCALE_CLOUD_INIT_DIR="/var/lib/libvirt/boot"
export BENCHSCALE_MAX_DISK_SIZE_GB=500
export BENCHSCALE_MIN_FREE_SPACE_GB=20

# Virtualization settings
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_DEFAULT_OS_VARIANT="ubuntu24.04"
export BENCHSCALE_VNC_BASE_PORT=5900
```

### Configuration file (`BenchScaleConfig`)

Runtime settings are merged from defaults, **environment variables** (see above), and an optional YAML file loaded via `BenchScaleConfig::from_file("benchscale.yaml")`. The same structure serializes to YAML; use `src/config/` for field documentation and validation.

---

## Usage

### Basic VM Creation

```rust
use benchscale::{LibvirtBackend, CloudInit, Backend};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create backend
    let backend = LibvirtBackend::new()?;
    
    // Generate cloud-init
    let cloud_init = CloudInit::builder()
        .add_user("testuser", "ssh-rsa AAAAB3...")
        .packages(vec!["vim".to_string(), "curl".to_string()])
        .build();
    
    // Create VM
    let node = backend.create_node(
        "my-vm",
        Path::new("/var/lib/libvirt/images/ubuntu-24.04.img"),
        2048,  // memory_mb
        2,     // vcpus
        20,    // disk_gb
        Some(&cloud_init),
    ).await?;
    
    println!("VM created: {} at {}", node.name, node.ip_address);
    Ok(())
}
```

### With Senescence Monitoring

```rust
use benchscale::config::{BenchScaleConfig, MonitoringConfig};

// Create config with cloud-init preset
let config = BenchScaleConfig {
    monitoring: MonitoringConfig::for_cloud_init_packages(),
    ..Default::default()
};

// Create monitor
let monitor = Arc::new(
    SenescenceMonitor::from_config(
        vm_name.clone(),
        node.ip_address.clone(),
        node.metadata.get("mac_address").cloned(),
        &config.monitoring,
    )
);

// Start monitoring
let handle = tokio::spawn({
    let monitor = monitor.clone();
    async move {
        monitor.start().await
    }
});

// Wait for cloud-init
tokio::select! {
    result = monitor.wait_for_cloud_init() => {
        result?;
        println!("Cloud-init complete!");
    }
    _ = tokio::time::sleep(config.timeouts.cloud_init()) => {
        anyhow::bail!("Cloud-init timeout!");
    }
}
```

---

## Recent Evolutions

### Evolution #23: Robust Package Verification ✅
**Status**: Complete & Validated  
**Impact**: Caught real installation failures

Multi-method verification system:
1. `dpkg-query` - Most reliable, direct package check
2. `dpkg -l` - Standard package listing
3. `apt-cache policy` - Repository and version check
4. Dependency check - Transitively installed packages

Architecture suffix handling for Ubuntu 24.04 packages (`:amd64`).

### Evolution #22: DHCP Lease Renewal Tracking ✅
**Status**: Complete & Validated  
**Impact**: Prevents false negatives during long builds

- MAC-based VM identification
- Periodic IP re-discovery (every 100 checks)
- Transparent to consumers
- Configuration: `ip_rediscovery_interval`

### Evolution #21: Configurable Failure Threshold ✅
**Status**: Complete & Validated  
**Impact**: 30-minute tolerance for cloud-init builds

- Configurable `max_failures` in `SenescenceMonitor`
- Workload presets (quick, desktop, cloud-init)
- Configuration: `MonitoringConfig::for_cloud_init_packages()`

### Evolution #20: Libvirt Health Check & Auto-Recovery ✅
**Status**: Complete & Validated  
**Impact**: Self-healing infrastructure

- Health check module with graceful degradation
- Sudo-free recovery via `virsh` API
- Orphan detection (disabled due to daemonization)
- `ensure_healthy()` API for all backends

---

## Code Structure

```
benchScale/src/
├── backend/
│   ├── mod.rs                  # Backend trait definition
│   ├── libvirt/
│   │   ├── mod.rs              # Libvirt backend orchestration
│   │   ├── vm_lifecycle.rs     # VM create/delete/list
│   │   ├── dhcp_discovery.rs   # MAC-based IP discovery
│   │   ├── health_check.rs     # Infrastructure health (Evolution #20)
│   │   ├── recovery.rs         # Auto-recovery logic (Evolution #20)
│   │   ├── boot_diagnostics.rs # Serial console, systemd logs (Evolution #13)
│   │   └── vm_guard.rs         # RAII cleanup pattern
│   ├── senescence.rs           # VM health monitoring (Evolutions #21, #22)
│   └── ssh.rs                  # SSH execution and validation
├── config/
│   ├── mod.rs                  # Top-level config (Phase 2A-C, 3A)
│   ├── monitoring.rs           # MonitoringConfig (Evolution #21)
│   ├── timeouts.rs             # TimeoutConfig
│   ├── network.rs              # NetworkConfig (Phase 2C)
│   ├── storage.rs              # StorageConfig (Phase 2C)
│   └── virtualization.rs       # VirtConfig (Phase 3A)
├── capabilities.rs             # Runtime discovery (Phase 3A)
├── cloud_init.rs               # Cloud-init generation
├── image_builder.rs            # Image operations (1143 lines - refactor target)
└── lib.rs                      # Public API
```

---

## Testing

### Run Tests

```bash
# All tests
cargo test

# With output
cargo test -- --nocapture

# Specific module
cargo test config::

# Integration tests
cargo test --test evolution_integration_tests
```

### Test Coverage

| Category | Tests | Status |
|----------|-------|--------|
| **Configuration** | 77 | ✅ 100% |
| **Monitoring** | 12 | ✅ 100% |
| **Health Check** | 6 | ✅ 100% |
| **DHCP Discovery** | 4 | ✅ 100% |
| **Integration** | 12 | ✅ 100% |
| **Unit Tests** | 101 | ✅ 100% |
| **Total** | **212** | ✅ **100%** |

---

## Best Practices

### Configuration

1. **Use workload presets**: `MonitoringConfig::for_cloud_init_packages()`
2. **Environment variables**: Override defaults without code changes
3. **Validate early**: `config.validate()` at startup
4. **Merge with capabilities**: `config.merge_with_capabilities(&caps)`

### Monitoring

1. **MAC-based tracking**: Always provide MAC address to `SenescenceMonitor`
2. **Appropriate timeouts**: Use presets for different workloads
3. **Check metrics**: Access `monitor.metrics()` for real-time status
4. **Handle IP changes**: Monitor automatically tracks DHCP renewals

### VM Lifecycle

1. **Use VmGuard**: Automatic cleanup on scope exit (RAII)
2. **Preserve on failure**: Set `PRESERVE_VM_ON_FAILURE=1` for debugging
3. **Health checks**: Call `backend.ensure_healthy()` before operations
4. **Boot diagnostics**: Available for failed VMs via `boot_diagnostics` module

---

## Known Issues & Roadmap

### 🟢 LOW: Image Builder Refactoring

**Current**: `image_builder.rs` (1143 lines) - mixed concerns  
**Target**: Smart cohesive module extraction
- `operations.rs` - Disk operations (~200 lines)
- `template.rs` - Template loading/validation (~150 lines)
- `validation.rs` - Image validation/checksums (~100 lines)
- `config.rs` - Builder configuration (~150 lines)
- `mod.rs` - Builder pattern, orchestration (~250 lines)

**Timeline**: 1-2 sessions  
**Risk**: LOW  
**Impact**: HIGH (maintainability, testability)

---

## Metrics

### Code Quality

| Metric | Value | Status |
|--------|-------|--------|
| **Tests** | 212/212 | ✅ 100% |
| **Unsafe code** | 0 | ✅ Enforced |
| **Production mocks** | 0 | ✅ Isolated |
| **Hardcoded values** | Minimal | ✅ Capability-based |
| **Large files** | 1 (image_builder.rs) | 🟡 Refactor planned |

### Evolution Status

| Evolution | Status | Impact |
|-----------|--------|--------|
| **#20: Health Check** | ✅ Complete | Self-healing |
| **#21: Configurable Thresholds** | ✅ Complete | 30min tolerance |
| **#22: DHCP Tracking** | ✅ Complete | False negative prevention |
| **#23: Robust Verification** | ✅ Complete | Real failure detection |

---

## Philosophy: Primal Architecture

benchScale embodies **primal principles**:

- **Self-Knowledge**: Components discover their own capabilities
- **Runtime Discovery**: `SystemCapabilities` for environment
- **Capability-Based**: No hardcoded assumptions
- **Fractal/Isomorphic**: Patterns consistent at all scales
- **Fast AND Safe**: Zero unsafe, zero-cost abstractions
- **Deep Debt Solutions**: Root causes, not bandaids

---

## Related Projects

- **[agentReagents](../agentReagents/)** - Template-driven VM image builder (consumer)
- **[ionChannel](../ionChannel/)** - Remote desktop portal and A/B testing
- **[ecoPrimals](../)** - Parent project and ecosystem

---

## Documentation

- [`ARCHITECTURE.md`](specs/ARCHITECTURE.md) - System architecture
- [`GUIDANCE.md`](specs/GUIDANCE.md) - Best practices
- [`../STATUS.md`](../STATUS.md) - Project-wide status
- [`../INDEX.md`](../INDEX.md) - Complete documentation index
- [`../EVOLUTION_*.md`](../) - Evolution documentation

---

## Support

- **Issues**: Document in root `STATUS.md` and evolution docs
- **Configuration**: See `src/config/` module documentation
- **Examples**: See `examples/` directory

---

**benchScale** - *Type-safe VM orchestration for modern infrastructure*

Production-ready • 212 tests passing • Zero unsafe code • Primal architecture

Made with 🦀 by the ecoPrimals ecosystem
