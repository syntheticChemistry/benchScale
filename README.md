# benchScale

**Pure Rust laboratory substrate for distributed system testing**

🟢 **Status**: Production Ready — v3.1.0, Rust 2024 edition  
📅 **Last Updated**: April 5, 2026  
🧪 **Tests**: 343 passing (7 ignored)  
🔒 **Safety**: `deny(unsafe_code)`, `deny(clippy::unwrap_used)`, `clippy::pedantic` + `clippy::nursery`  
📜 **License**: AGPL-3.0-or-later (scyBorg Provenance Trio)

---

## What is benchScale?

benchScale is a pure Rust framework for creating reproducible, isolated test environments for distributed systems. It provides Docker and libvirt backends, network simulation with named presets, topology-driven lab creation, and a JSON-RPC 2.0 server mode for programmatic control.

**Ecosystem role**: Infrastructure tooling for the ecoPrimals ecosystem. Used by biomeOS for VM federation testing, primalSpring for validation pipelines, and hotSpring for GPU sovereign compute isolation.

**Deep debt resolution sprint (v3.1.0)**: Raised line coverage from ~35.5% to ~61.9%, expanded the library test suite to 343 tests, tightened unsafe boundaries (EnvGuard, safe FFI, `nix` for signals), upgraded `thiserror` to 2.x, refactored modules (`vm_state`, `stages`, `config/legacy`), aligned lint policy on `#[expect(...)]` with reasons, and set `tarpaulin` fail-under 60%. See `CHANGELOG.md` for the full list.

## Features

- **Docker + Libvirt backends** with a shared `Backend` trait
- **Topology-driven labs** from YAML manifests
- **Network simulation** with 5 named presets (basement_lan, campus, broadband, cellular, satellite)
- **JSON-RPC 2.0 server** (`benchscale server --port PORT`) per UniBin standard
- **IPC compliance validation** (`benchscale validate ipc ENDPOINT`)
- **Cross-architecture binary resolution** for plasmidBin deployment
- **Cloud-init generation** with type-safe builder
- **VM senescence monitoring** with DHCP lease tracking
- **Self-healing infrastructure** with health checks and auto-recovery

## Quick Start

```bash
cargo build --release
cargo test

# Create a lab from a topology
cargo run -- create my-lab topologies/simple-lan.yaml

# Start JSON-RPC server
cargo run -- server --port 9200

# Validate IPC compliance of a running primal
cargo run -- validate ipc 127.0.0.1:9100
```

## JSON-RPC Methods

When running in server mode (`benchscale server --port PORT`):

| Method | Description |
|--------|-------------|
| `health.liveness` | Mandatory liveness probe |
| `health.readiness` | Readiness with Docker status |
| `health.check` | Full health with lab count |
| `lab.create` | Create lab from topology (supports `plasmid_bin_path`) |
| `lab.destroy` | Tear down a lab |
| `lab.list` | List all active labs |
| `lab.status` | Detailed lab status |
| `topology.validate` | Validate topology without deploying |
| `node.health` | Probe a specific lab node |

## Code Structure

```
src/
├── backend/
│   ├── mod.rs              # Backend trait
│   ├── docker.rs           # Docker backend (bollard)
│   ├── libvirt/             # Libvirt/KVM backend
│   ├── senescence.rs       # VM health monitoring
│   ├── ssh.rs              # SSH execution
│   └── cleanup.rs          # Resource cleanup
├── config/                  # Type-safe configuration
├── deploy/
│   ├── arch.rs             # Cross-arch binary resolution
│   └── plasmid.rs          # plasmidBin integration
├── server/
│   ├── mod.rs              # JSON-RPC 2.0 TCP server
│   └── methods.rs          # Method handlers
├── validation/
│   └── ipc_compliance.rs   # IPC compliance testing
├── lab/                     # Lab lifecycle management
├── topology/                # YAML topology parsing
├── network/                 # Network simulation presets
├── image_builder/           # VM image building pipeline
├── cloud_init.rs            # Cloud-init generation
├── scenarios/               # Test scenario runner
└── lib.rs                   # Public API
```

## Configuration

Environment variables override defaults:

```bash
BENCHSCALE_SSH_PORT=22
BENCHSCALE_LIBVIRT_URI="qemu:///system"
BENCHSCALE_NETWORK_NAME="default"
BENCHSCALE_VM_IMAGES_DIR="/var/lib/libvirt/images"
```

See `src/config/` for the full `BenchScaleConfig` system with validation, workload presets, and YAML serialization.

## Related Projects

- **[agentReagents](../agentReagents/)** — Template-driven VM image builder
- **[wateringHole](../wateringHole/)** — ecoPrimals inter-project standards
- **[plasmidBin](../plasmidBin/)** — Binary distribution for primals

## License

AGPL-3.0-or-later — Part of the ecoPrimals ecosystem.

---

Made with Rust by the ecoPrimals ecosystem
