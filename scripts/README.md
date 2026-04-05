# benchScale shell scripts

## Current (maintained)

| Script | Role |
|--------|------|
| `deploy-ecoprimals.sh` | Deploy plasmidBin binaries and graphs into a running lab (ecoPrimals integration). |
| `run-tests.sh` | Delegates to primalSpring’s lab validation; use for ecosystem experiments against a live lab. |
| `create-lab.sh` | Bash helper to create labs (Docker/LXD/QEMU paths); for the canonical path prefer `cargo run -- create …` / `benchscale create`. |
| `destroy-lab.sh` | Bash helper to tear down labs; canonical CLI: `cargo run -- destroy …` / `benchscale destroy`. |

## Archive (`scripts/archive/`)

| Script | Role |
|--------|------|
| `run-tests-stub.sh` | Old placeholder test runner; superseded by `run-tests.sh`. |
| `download-isos-legacy.sh` | Legacy LXC-based primal deployment (filename is historical; does not download ISOs). Superseded by `deploy-ecoprimals.sh` and the Rust CLI for most workflows. |
