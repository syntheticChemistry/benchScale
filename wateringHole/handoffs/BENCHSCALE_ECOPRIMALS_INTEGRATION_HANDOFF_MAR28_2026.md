# benchScale — ecoPrimals Integration Handoff

**Date**: March 28, 2026
**Version**: 2.1.0 (live-tested)
**Author**: Agent-assisted
**Scope**: Integration of benchScale as local validation substrate for ecoPrimals primal compositions

---

## What Was Done

### Topology YAMLs (3 files in `topologies/`)

- **ecoprimals-tower-2node.yaml** — Minimal 2-node: Tower (beardog + songbird + biomeOS) and spring primals (groundspring, healthspring_primal, neuralspring, wetspring, ludospring). Home LAN conditions (2ms, 1Gbps).
- **ecoprimals-nucleus-3node.yaml** — Full 3-node NUCLEUS: all 6 core primals, 5 spring primals, and a degraded mobile-like node behind NAT. Mixed conditions: basement_lan, home_lan, mobile_cell.
- **ecoprimals-wan-federation.yaml** — WAN stress test: same nodes with friend_wan, mobile_cell, satellite presets. Tests BirdSong and federation under real-world degradation.

### Network Presets (`topologies/network-presets.yaml`)

5 presets mapping to real deployment scenarios: basement_lan (0.5ms/10Gbps), home_lan (2ms/1Gbps), friend_wan (50ms/100Mbps), mobile_cell (100ms/10Mbps), satellite (600ms/5Mbps). Each includes jitter values.

### Deploy Script (`scripts/deploy-ecoprimals.sh`)

Orchestrates binary deployment into benchScale lab nodes:
- Parses topology YAML for node names and PRIMALS env metadata (fixed multi-word value extraction)
- Copies musl static binaries from plasmidBin into `/opt/ecoprimals/bin/`
- **Per-primal launch commands** matching actual CLI interfaces:
  - beardog: `server --listen 0.0.0.0:PORT --family-id ID`
  - songbird: `server --port PORT` (HTTP, not raw TCP JSON-RPC)
  - biomeos: `neural-api` subcommand
  - neuralspring/healthspring_primal: `serve` subcommand
  - groundspring/wetspring/ludospring: `server` subcommand
- Builds primal-specific environment variables (BEARDOG_SOCKET, SONGBIRD_SECURITY_PROVIDER, etc.)
- Health checks use TCP JSON-RPC for beardog, HTTP /health for songbird
- Works with Docker and LXD hypervisors

### create-lab.sh

- **Generic YAML-driven Docker provisioning** — replaced hardcoded topology handlers
- Parses node names, images, env vars, network conditions from any topology YAML
- Creates Docker network + containers with NET_ADMIN for tc
- Records container IDs for reliable teardown via destroy-lab.sh

### Experiments Updated

- **exp074_cross_gate_health**: Added HTTP health protocol for songbird (dual TCP/HTTP probe)
- **exp073_lan_covalent_mesh**: Added HTTP health check for songbird discovery port

### Documentation (ecoPrimals standard)

- `CONTEXT.md` — What/Role/Architecture/Boundaries/Status/Ecosystem
- `specs/ECOPRIMALS_INTEGRATION.md` — Full integration spec with gap analysis
- `wateringHole/` — This handoff structure

### plasmidBin Integration

- `deploy_gate.sh` gains `--local-validate` flag: runs benchScale Docker validation before SSH deploy
- Auto-selects topology based on composition (tower -> tower-2node, full -> nucleus-3node)

## Live Test Results (March 28, 2026)

### End-to-End Pipeline: PASS

```
Topology:   ecoprimals-tower-2node
Hypervisor: docker
Experiments: 2 pass, 0 fail, 0 skip
```

### Primal Status in Docker Containers

| Primal | Node | Status | Protocol |
|--------|------|--------|----------|
| beardog | node-tower | LIVE (TCP 9100) | JSON-RPC |
| songbird | node-tower | LIVE (HTTP 9200) | HTTP /health |
| biomeos | node-tower | **FIXED** (was ZOMBIE) | JSON-RPC TCP — deploy script now passes `--graphs-dir`, `--port`, `--family-id` |
| groundspring | node-spring | Running (UDS) | Unix socket |
| healthspring_primal | node-spring | Running (UDS) | Unix socket |
| neuralspring | node-spring | Running (UDS) | Unix socket |
| wetspring | node-spring | Running (UDS) | Unix socket |
| ludospring | node-spring | Running (UDS) | Unix socket |

- **8 of 8 primals expected alive** in Docker (biomeos ZOMBIE **FIXED** — deploy script corrected April 7)
- beardog + songbird reachable from host via Docker network IP
- beardog responds to `health.liveness` and `capabilities.list` JSON-RPC
- songbird responds to HTTP `GET /health` with `OK`

### Experiment Results

**exp074_cross_gate_health**: 6/6 PASS (7 skipped — nestgate/toadstool/squirrel not in tower topology)
- beardog LIVE, songbird LIVE, Tower Atomic composition confirmed

**exp073_lan_covalent_mesh**: 3/3 PASS (5 skipped — mesh/birdsong need IPC socket access)
- beardog LIVE, songbird LIVE (HTTP), remote_gate_configured

## Resolved Gaps (from v2.0.0)

1. ~~create-lab.sh hardcoded topology handlers~~ → Generic YAML-driven Docker path
2. ~~deploy-ecoprimals.sh bare server --listen~~ → Per-primal launch commands matching CLI
3. ~~FAMILY_ID mismatch~~ → Extracted from topology YAML consistently
4. ~~plasmidBin binaries not harvested~~ → All 13 binaries present (7 primals + 6 springs)
5. ~~YAML multi-word value extraction~~ → Fixed awk to capture full values

## Remaining Gaps

1. ~~**biomeos** crashes (`neural-api` needs graph/biome.yaml to initialize)~~ **RESOLVED** (April 7) — `deploy-ecoprimals.sh` now passes `--graphs-dir $DEPLOY_DIR/graphs --port $port --family-id '$family_id'` to biomeOS. Health check upgraded from 5s single-shot to 15s grace + 3 retries with 10s intervals.
2. **Spring primals** listen on UDS only (no TCP health check from host)
3. **mesh.peers** requires multiple Songbird instances or IPC socket access
4. **Songbird JSON-RPC** only accessible via Unix socket, not HTTP port
5. **Network conditions (tc)** not applied in Docker (containers lack iproute2)
