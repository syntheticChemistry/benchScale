# benchScale — ecoPrimals Integration Specification

**Date**: March 28, 2026
**Status**: Phase 2 — Live-tested, end-to-end pipeline functional (2 experiments pass)

---

## Overview

benchScale serves as the local validation substrate for ecoPrimals primal
compositions. This spec documents the integration surface between benchScale
and the wider ecosystem: what topologies exist, how binaries flow from
plasmidBin into lab nodes, and what gaps remain before end-to-end automated
validation works.

## Topology Inventory

| Topology | Nodes | Presets | Purpose |
|----------|-------|---------|---------|
| `ecoprimals-tower-2node` | 2 (tower + springs) | `home_lan` | Minimal cross-gate validation |
| `ecoprimals-nucleus-3node` | 3 (nucleus + springs + mobile) | `basement_lan`, `home_lan`, `mobile_cell` | Full NUCLEUS + NAT traversal |
| `ecoprimals-wan-federation` | 3 (nucleus + springs + mobile) | `friend_wan`, `mobile_cell`, `satellite` | WAN degradation + BirdSong resilience |

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

The shell script path has **hardcoded case branches** for only 3 legacy
topologies (`simple-lan`, `p2p-3-tower`, `nat-traversal`). Custom topologies
fall through to a no-op warning. This must be updated with a generic Docker
path that parses YAML and creates containers dynamically.

**Decision**: `create-lab.sh` must gain a generic Docker path OR
`validate_local_lab.sh` must call the `benchscale` Rust CLI directly.

## Known Integration Gaps

### 1. Shell create-lab.sh does not handle ecoPrimals topologies

**Impact**: `validate_local_lab.sh` calls shell scripts, which create no containers.
**Fix**: Add generic Docker/YAML parsing to `create-lab.sh`.

### 2. Deploy script does not wire launch profile environment

**Impact**: `deploy-ecoprimals.sh` starts primals with bare `server --listen`
but does not set `BEARDOG_SOCKET`, `SONGBIRD_SECURITY_PROVIDER`, or other
env vars from `primal_launch_profiles.toml`. BirdSong beacon generation
requires BearDog connectivity.
**Fix**: Parse launch profiles and set env vars per primal on startup.

### 3. FAMILY_ID mismatch

**Impact**: Topology YAMLs set `FAMILY_ID: "benchscale-tower-2node"`, but
`validate_local_lab.sh` exports `FAMILY_ID="$LAB_NAME"`. BirdSong beacon
family check fails.
**Fix**: Use topology's FAMILY_ID consistently, or pass `--seed` matching the YAML.

### 4. mesh.peers requires multiple Songbird instances

**Impact**: exp073 validates `mesh.peers >= 1`. `ecoprimals-tower-2node`
only runs Songbird on `node-tower`. No peer discovery possible.
**Fix**: Add Songbird to `node-spring` topology, or create a dedicated mesh topology.

### 5. No harvested binaries in plasmidBin

**Impact**: `deploy-ecoprimals.sh` logs "binary not found" for all primals.
**Fix**: Run `harvest.sh` or manually populate `plasmidBin/primals/`.

## Validation Matrix

| Experiment | What it validates | Works in benchScale lab? | Blocking gap |
|------------|-------------------|--------------------------|--------------|
| exp074 | Cross-gate NUCLEUS health | Yes (once binaries harvested) | Gap 5 |
| exp073 | BirdSong beacon + mesh | Partial (beacon yes, mesh no) | Gaps 2, 3, 4 |
| exp076 | Neural routing cross-gate | No (hardcoded localhost) | Code change needed |
| exp063 | Pixel Tower rendezvous | No (ADB-specific) | Out of scope |

## Phased Roadmap

**Phase 1** (current): Topology YAMLs + deploy script + validate_local_lab.sh
**Phase 2**: Fix gaps 1-4, achieve exp073 + exp074 passing in Docker lab
**Phase 3**: libvirt VM path with agentReagents cloud-init, full OS fidelity
**Phase 4**: Network preset sweep — run validation across all 5 presets
