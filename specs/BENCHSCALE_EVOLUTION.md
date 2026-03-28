# benchScale Evolution Specification

**Date**: March 28, 2026
**Version**: 3.0.0 (target)
**Status**: Specification — phased execution roadmap
**Current**: 2.1.0 (live-tested Docker labs, 7/8 primals, 2/2 experiments pass)

---

## Purpose

Define the tractable evolution path from benchScale 2.1.0 (functional but internally inconsistent) to 3.0.0 (robust deployment validation substrate for ecoPrimals). Each phase is independently valuable and can be completed in a single session.

## Architecture: Current vs Target

### Current (2.1.0)

Two parallel execution paths that don't share state:

- **Shell path** (`scripts/`): `create-lab.sh` -> `deploy-ecoprimals.sh` -> `destroy-lab.sh`. State in `.state/<lab>/info.yaml` + `nodes.txt`. Works. Used by `validate_local_lab.sh`.
- **Rust path** (`src/bin/main.rs`): `benchscale create` -> bollard Docker API. State in `~/.benchscale/labs/*.json`. Broken destroy (node_ids never populated in registry after create).

### Target (3.0.0)

Single canonical path: **shell scripts for orchestration, Rust library for backends**.

The shell scripts work, are tested against real Docker labs, and integrate with primalSpring's validation pipeline. The Rust library provides typed backends (Docker, libvirt) that shell scripts can optionally delegate to via the CLI. The CLI becomes a thin wrapper around the library, using the same `.state/` directory.

---

## Phase A: Cleanup (no behavior changes)

### A1. Rename misleading scripts

`scripts/deploy-to-lab.sh` is actually an ISO download script (wget for Pop!_OS, Ubuntu). It has nothing to do with lab deployment.

- Rename to `scripts/archive/download-isos-legacy.sh`
- Create `scripts/archive/` directory for scripts that are kept but not part of the active path
- Fix the "next steps" output in `create-lab.sh` that references `deploy-to-lab.sh`

### A2. Replace fake test runner

`scripts/run-tests.sh` (198 lines) contains only sleep + fake success output for named "tests". It gives false confidence.

- Replace with a thin wrapper that calls `validate_local_lab.sh` with the lab's topology
- Or delete entirely and document that primalSpring experiments are the test surface

### A3. Remove orphan Rust source files

Two files exist in `src/` but are not referenced from `lib.rs`:

- `src/image_builder_improvements.rs` (348 lines) — improvement proposals, not compiled
- `src/cloud_init_tests.rs` (93 lines) — test code, not compiled

Move to `archive/rust/` to preserve history without polluting the source tree.

### A4. Fix `pub mod tests` naming collision

`src/lib.rs` line 73 declares `pub mod tests` which exports `TestRunner`, `TestScenario`, `TestResult`. This collides with `#[cfg(test)] mod tests` convention and confuses tooling.

- Rename module to `pub mod scenarios` (or `pub mod test_runner`)
- Update re-exports on line 90: `pub use scenarios::{TestResult, TestRunner, TestScenario};`
- Update any imports in `src/bin/main.rs` or tests

### A5. Update stale spec

`specs/ECOPRIMALS_INTEGRATION.md` lists "Gap 1: create-lab.sh has hardcoded topology handlers" which was fixed in v2.1.0. Update the spec to reflect current state and point at this evolution doc for forward work.

---

## Phase B: Unify State Model

### B1. Canonical state directory

**Decision: `.state/` wins.** It's in the repo (gitignored), co-located with scripts, and proven in production.

Schema per lab:
```
.state/<lab-name>/
  info.yaml          # topology, hypervisor, creation time
  nodes.txt          # container/VM IDs (one per line)
  network.txt        # network ID
  deploy.log         # last deploy output
  health.log         # last health check
```

### B2. Rust CLI uses `.state/`

Modify `src/bin/main.rs` to:
- Default `BENCHSCALE_STATE_DIR` to `<repo>/.state/` (detected from binary location or CWD)
- After `Lab::create`, write `info.yaml` + `nodes.txt` + `network.txt` in the same format as shell scripts
- `benchscale destroy` reads from `.state/<name>/` and actually cleans up
- `benchscale list` reads `.state/*/info.yaml`
- `benchscale status <name>` reads `.state/<name>/` and probes Docker

### B3. Wire clap

Replace manual `std::env::args()` parsing with clap (already a dev-dependency, move to main deps):

```
benchscale create --topology <name> --name <lab> [--hypervisor docker|lxd|qemu]
benchscale destroy --lab <name> [--force]
benchscale list
benchscale status <name>
benchscale deploy --lab <name> --plasmidbin <path> [--graphs <path>]
benchscale version
```

The `deploy` subcommand is new — it wraps `deploy-ecoprimals.sh` logic in Rust, or shells out to it initially.

---

## Phase C: Deployment Validation Matrix

### C1. Current coverage

| Topology | Preset | Bond | Status |
|----------|--------|------|--------|
| tower-2node | home_lan | covalent | LIVE (2/2 exp pass) |
| nucleus-3node | mixed | covalent | Defined, not live-tested |
| wan-federation | degraded | covalent | Defined, not live-tested |

### C2. Target coverage (priority order)

| Topology | Preset | Bond | Experiments | Priority |
|----------|--------|------|-------------|----------|
| tower-2node | home_lan | covalent | exp073, exp074 | Done |
| nucleus-3node | basement+home+mobile | covalent | exp074 (full NUCLEUS) | P1 |
| tower-2node | mobile_cell | covalent | exp073, exp074 (degraded) | P2 |
| wan-federation | friend+mobile+satellite | covalent | exp073 (federation stress) | P2 |
| tower-2node | home_lan | ionic | new: exp077 (cross-family) | P3 |
| nucleus-3node | home_lan | metallic | new: exp078 (fleet compute) | P4 |

### C3. Cross-arch binary resolution

Add `primals/aarch64/` and `springs/aarch64/` to the binary search path in `deploy-ecoprimals.sh`:

```bash
for candidate in \
    "$PLASMIDBIN_DIR/$primal" \
    "$PLASMIDBIN_DIR/primals/$primal" \
    "$PLASMIDBIN_DIR/springs/$primal" \
    "$PLASMIDBIN_DIR/primals/aarch64/$primal" \
    "$PLASMIDBIN_DIR/springs/aarch64/$primal" \
    "$PLASMIDBIN_DIR/bin/$primal"; do
```

For aarch64 Docker labs, document the `--platform linux/arm64` + qemu-user-static path.

### C4. Network enforcement

Docker containers need `iproute2` for `tc` rules. Two options:

**Option 1 (recommended)**: Install at container creation in `create-lab.sh`:
```bash
docker exec "$container_name" sh -c "apt-get update -qq && apt-get install -y -qq iproute2" 2>/dev/null
```

**Option 2**: Build a custom `benchscale-node:24.04` image with iproute2 pre-installed.

Option 1 is simpler and avoids maintaining a custom image. The 2-3 second overhead is acceptable for lab creation.

---

## Phase D: IPC Compliance Integration

### D1. Launch profile awareness

`deploy-ecoprimals.sh` currently hardcodes per-primal launch commands in `build_launch_cmd()`. This should be data-driven.

**Short term**: Keep the case statement but document which IPC Compliance Matrix entries it relies on.

**Medium term**: Parse `primal_launch_profiles.toml` in the deploy script to derive:
- Subcommand (`server`, `serve`, `daemon`, `neural-api`)
- Port flag (`--listen`, `--port`, or env-only)
- Family ID passing (`--family-id` CLI, `FAMILY_ID` env, or profile-specific like `NESTGATE_FAMILY_ID`)
- Socket wiring (env vars for inter-primal dependencies)

### D2. Health probe protocol awareness

`health_check_node()` currently uses TCP JSON-RPC for beardog and HTTP for songbird. This should be driven by a primal protocol table:

```bash
# Protocol per primal (from IPC Compliance Matrix)
primal_protocol() {
    case "$1" in
        beardog|nestgate|toadstool|squirrel) echo "tcp_jsonrpc" ;;
        songbird)                             echo "http" ;;
        *)                                    echo "uds" ;;
    esac
}
```

### D3. TCP-first vs UDS-first testing

gen4 products use TCP-first composition. gen3 springs use UDS-first. Labs should support both:

- Default: TCP-first (current behavior — primals bind TCP ports)
- Flag: `--uds-first` to skip TCP binding and rely on Unix sockets within containers
- This validates that compositions work in both orders, matching IPC_COMPLIANCE_MATRIX transport tiers

---

## Dead Code Inventory

| File | Lines | Status | Action |
|------|-------|--------|--------|
| `src/image_builder_improvements.rs` | 348 | Orphan (not in lib.rs) | Move to archive/ |
| `src/cloud_init_tests.rs` | 93 | Orphan (not in lib.rs) | Move to archive/ |
| `scripts/deploy-to-lab.sh` | 160 | Misnamed ISO downloader | Move to archive/ |
| `scripts/run-tests.sh` | 198 | Fake test output | Replace or delete |

## Success Criteria

- **Phase A**: `cargo check` clean, no orphan files in src/, no misleading scripts, spec reflects reality
- **Phase B**: `benchscale create` and `benchscale destroy` use `.state/`, shell and Rust paths interoperate
- **Phase C**: nucleus-3node live-tested, tc rules applied, cross-arch binaries found
- **Phase D**: deploy script reads launch profiles, health probes use correct protocol per primal
