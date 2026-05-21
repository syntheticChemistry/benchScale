# benchScale — ecoPrimals Integration Specification

**Date**: April 13, 2026
**Status**: Phase 3 — All 5 integration gaps resolved, musl binaries harvested, Docker lab pipeline end-to-end

---

## Overview

benchScale serves as the local validation substrate for ecoPrimals primal
compositions. This spec documents the integration surface between benchScale
and the wider ecosystem: what topologies exist, how binaries flow from
plasmidBin into lab nodes, and what gaps remain before end-to-end automated
validation works.

## Topology Inventory

21 ecoPrimals topologies available, from minimal 2-node to 10-node federation:

| Topology | Nodes | Presets | Purpose |
|----------|-------|---------|---------|
| `ecoprimals-tower-2node` | 2 (tower + springs) | `home_lan` | Minimal cross-gate validation |
| `ecoprimals-tower-2node-tcp` | 2 (tower + springs) | `home_lan` | TCP-only variant |
| `ecoprimals-nucleus-3node` | 3 (nucleus + springs + mobile) | `basement_lan`, `home_lan`, `mobile_cell` | Full NUCLEUS + NAT traversal |
| `ecoprimals-wan-federation` | 3 (nucleus + springs + mobile) | `friend_wan`, `mobile_cell`, `satellite` | WAN degradation + BirdSong resilience |
| `ecoprimals-albatross-multiplex` | 3+ (multi-Songbird mesh) | `home_lan` | BirdSong mesh + peer discovery |

## Binary Flow

```
primal repos (barraCuda, songbird, beardog, ...)
    │  cargo build --release --target x86_64-unknown-linux-musl
    ▼
plasmidBin/primals/{beardog,songbird,...}    ← musl static binaries
plasmidBin/springs/{groundspring,...}        ← spring primal binaries
    │  deploy-ecoprimals.sh --plasmidbin ...
    ▼
benchScale lab node: /opt/ecoprimals/bin/   ← inside container or VM
    │  nohup ./beardog server --listen 0.0.0.0:9100 ...
    ▼
TCP JSON-RPC on ports 9100-9800             ← experiments connect here
```

## Network Presets

Defined in `topologies/network-presets.yaml`. Map to real deployment scenarios:

| Preset | Latency | Loss | Bandwidth | Scenario |
|--------|---------|------|-----------|----------|
| `basement_lan` | 0.5ms | 0% | 10 Gbps | Same-rack covalent mesh |
| `home_lan` | 2ms | 0.01% | 1 Gbps | Home network gates |
| `friend_wan` | 50ms | 0.5% | 100 Mbps | Friend remote covalent |
| `mobile_cell` | 100ms | 2% | 10 Mbps | Pixel over cellular |
| `satellite` | 600ms | 5% | 5 Mbps | Remote/rural gate |

## Two Execution Paths

### Rust CLI (`benchscale create`)

The `Lab::create` path in `src/lab/mod.rs` is **generic and topology-driven**.
It parses any valid YAML, creates a Docker network, iterates `topology.nodes`,
and creates one container per node via `DockerBackend::create_node`. This is
the correct path for ecoPrimals topologies.

### Shell scripts (`create-lab.sh`)

`create-lab.sh` now has **generic YAML-driven Docker creation** with minimal
YAML parsing helpers that extract node names, images, env vars, and network
conditions from any benchScale topology YAML. Both legacy topologies and
ecoPrimals topologies are handled by the same generic path.

## Integration Gaps (all resolved — Phase 41)

### 1. ~~Shell create-lab.sh does not handle ecoPrimals topologies~~ RESOLVED

`create-lab.sh` now has generic YAML-driven Docker creation that parses any
topology YAML and creates containers dynamically. No special-casing needed.

### 2. ~~Deploy script does not wire launch profile environment~~ RESOLVED

`deploy-ecoprimals.sh` now has `build_primal_env()` for per-primal TCP env
wiring and `apply_launch_profile()` which reads `primal_launch_profiles.toml`
and merges `extra_env` entries (e.g., `SWEETGRASS_MODE`, `LOAMSPINE_MODE`).

### 3. ~~FAMILY_ID mismatch~~ RESOLVED

`deploy_node()` now reads per-node `FAMILY_ID` from the topology YAML and
writes it to `.family.seed`. `start_node_primals()` reads per-node
`FAMILY_ID` and propagates to all primal-specific env vars. Consistent end-to-end.

### 4. ~~mesh.peers requires multiple Songbird instances~~ RESOLVED

`ecoprimals-albatross-multiplex.yaml` provides 3 Songbird instances for
mesh/peer discovery validation. 21 topologies available covering all scenarios.

### 5. ~~No harvested binaries in plasmidBin~~ RESOLVED

`plasmidBin/primals/` now contains all 13 NUCLEUS primal binaries (plus
skunkBat). `build_ecosystem_genomeBin.sh --harvest` builds musl-static and
populates via `harvest.sh`. Dynamic-linked release binaries also available
as fallback for local Docker testing.

## Validation Matrix

| Experiment | What it validates | Works in benchScale lab? | Notes |
|------------|-------------------|--------------------------|-------|
| exp074 | Cross-gate NUCLEUS health | Yes | All gaps resolved |
| exp073 | BirdSong beacon + mesh | Yes (full mesh with albatross topology) | Gaps 2-4 resolved |
| exp030 | Covalent bond | Partial (Tower probes pass, multi-node needs 2 labs) | |
| exp031 | Ionic bond | Yes (with `ecoprimals-ionic-2family` topology) | |
| exp034 | Capability aggregation | Yes (2+ live primals per node) | |
| exp076 | Neural routing cross-gate | Partial (needs REMOTE_GATE_HOST env) | |
| exp063 | Pixel Tower rendezvous | No (ADB-specific) | Out of scope |

## Phased Roadmap

**Phase 1** (complete): Topology YAMLs + deploy script + validate_local_lab.sh
**Phase 2** (complete): All 5 gaps resolved, end-to-end Docker pipeline functional
**Phase 3** (current): Live validation — run bonding experiments against Docker labs
**Phase 4**: libvirt VM path with agentReagents cloud-init, full OS fidelity
**Phase 5**: Network preset sweep — run validation across all 5 presets
