# benchScale — wateringHole

**Status**: v3.0.0 — Modernized Rust crate, clap CLI, capability-based config, 212 tests — March 28, 2026

---

## Role

benchScale is the ecoPrimals lab substrate: it turns declarative YAML topologies into runnable Docker (and optionally libvirt) environments, applies network conditions, and supports deploy-and-health workflows used by primalSpring validation.

## Current capabilities

- Docker labs from YAML topologies
- YAML topology definitions and validation
- Network presets (latency, loss, bandwidth, jitter via tc)
- Deploy + health pipeline for multi-node checks
- primalSpring experiment integration (local validation against composed gates)

## Active handoffs

| File | Scope |
|------|--------|
| [BENCHSCALE_ECOPRIMALS_INTEGRATION_HANDOFF_MAR28_2026.md](handoffs/BENCHSCALE_ECOPRIMALS_INTEGRATION_HANDOFF_MAR28_2026.md) | ecoPrimals topology, deploy, and validation substrate |

## Ecosystem flow

```
benchScale topologies → Docker labs → plasmidBin binaries → primalSpring experiments
```

## Convention

**Naming**: `BENCHSCALE_V{VERSION}_{TOPIC}_HANDOFF_{DATE}.md`

**Flow**: benchScale supplies reproducible multi-node environments; plasmidBin supplies binaries; primalSpring supplies experiment harnesses and assertions.
