# SPDX-License-Identifier: AGPL-3.0-only

# benchScale — Context

## What

benchScale is a pure Rust laboratory substrate for distributed system testing.
It creates isolated multi-node environments (Docker containers or libvirt VMs)
from declarative YAML topologies, provisions them via cloud-init, applies
network conditions (latency, loss, bandwidth via tc), and orchestrates
test workloads across them.

## Role

Infrastructure tooling for the ecoPrimals ecosystem. benchScale bridges the
gap between single-host testing (primalSpring's AtomicHarness with Unix
sockets) and real multi-gate deployment (physical machines on LAN/WAN).
It provides automated, reproducible multi-node environments on a single
dev machine for validating primal compositions, cross-gate health, BirdSong
mesh federation, and network degradation resilience.

## Architecture

- **src/backend/** — `Backend` trait + implementations: `DockerBackend` (default), `LibvirtBackend` (optional, KVM/QEMU), SSH backend for remote machines
- **src/topology/** — YAML topology parser: `Topology`, `NodeConfig`, `NetworkConditions`, validation
- **src/lab/** — `Lab` lifecycle: create network, create N nodes from topology, apply conditions, registry
- **src/network/** — `NetworkSimulator` for tc-based traffic shaping (latency, loss, bandwidth, jitter)
- **src/cloud_init.rs** — Type-safe cloud-init config builder for VM provisioning
- **src/persistence/** — SQLite-backed lab state persistence (optional `persistence` feature)
- **src/bin/main.rs** — `benchscale` **clap**-based CLI: `create`, `destroy`, `list`, `status`, and related subcommands
- **topologies/** — YAML topology definitions including ecoPrimals-specific compositions
- **scripts/** — Shell scripts for lab lifecycle (create, deploy, destroy, test)
- **specs/** — Architecture and guidance documentation
- **archive/** — Historical session docs and superseded code

## Key Features

| Feature | Default | Optional |
|---------|---------|----------|
| Docker containers | `docker` (default) | — |
| libvirt KVM/QEMU VMs | — | `libvirt` |
| Network simulation (tc) | Yes | — |
| Cloud-init provisioning | Yes | — |
| Lab persistence (SQLite) | — | `persistence` |
| SSH remote backend | — | `libvirt` |

## Boundaries

- benchScale does NOT run primals itself — it creates the environment, deploys binaries, and provides exec/copy primitives. Primal lifecycle is the responsibility of deploy scripts or the caller.
- benchScale does NOT parse primalSpring's `primal_launch_profiles.toml` — deploy scripts translate profiles into env vars and CLI args.
- benchScale does NOT replace the `benchscale` Rust CLI with shell scripts — the Rust `Lab::create` path is the generic, topology-driven path. Shell scripts are operational wrappers.

## IPC / Integration

- **Topology YAML** — declarative node definitions consumed by both Rust `Topology::from_file` and shell scripts
- **`.state/` directory** — lab metadata (info.yaml) shared between create and deploy scripts
- **Container exec** — `docker exec` / `lxc exec` for in-node commands
- **Container copy** — `docker cp` / `lxc file push` for binary deployment
- **TCP ports 9100-9800** — primal JSON-RPC endpoints within lab nodes (matching plasmidBin/ports.env)

## Status

Version 3.0.0 — Modernized Rust core: **clap** CLI, **capability-based config** (env overrides for paths, networks, and defaults), `yaml_serde` for YAML, unified `tracing` logging, and **212** tests. Docker and optional libvirt backends; ecoPrimals integration includes topology definitions, deploy scripts, and network presets. Shell script `create-lab.sh` supports the Docker path for arbitrary topologies.

## Ecosystem Position

```
infra/benchScale     — THIS: lab substrate (creates environments)
infra/agentReagents  — base images, ISOs, cloud-init configs
infra/plasmidBin     — primal binaries, manifests, checksums
springs/primalSpring — validation experiments that RUN in benchScale labs
```

benchScale + agentReagents provide the environment.
plasmidBin provides the binaries.
primalSpring provides the validation logic.
