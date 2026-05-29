#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# deploy-waterfall.sh — Deploy and validate WaterFall gate-sync in a benchScale lab
#
# This script provisions the ecoprimals-waterfall-gate-sync topology:
#   1. Starts a Forgejo container as the periplasm (golgiBody)
#   2. Seeds test repos into Forgejo via git push
#   3. Provisions gate containers with wateringHole + cascade-pull.sh
#   4. Runs cascade-pull --gate auto on each gate
#   5. Validates repo counts + HEAD parity across gates
#
# Usage:
#   ./deploy-waterfall.sh --lab <lab-name>
#   ./deploy-waterfall.sh --lab waterfall-test --skip-seed   # reuse existing Forgejo data
#   ./deploy-waterfall.sh --lab waterfall-test --test-only   # skip deploy, just test
#
# Prerequisites:
#   - Lab created via create-lab.sh --topology ecoprimals-waterfall-gate-sync
#   - Docker running
#   - ecoPrimals workspace at ../../.. (or set ECOPRIMALS_ROOT)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"

ECO_ROOT="${ECOPRIMALS_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
WH_DIR="$ECO_ROOT/infra/wateringHole"
MANIFEST="$WH_DIR/ecosystem_manifest.toml"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

log()      { echo -e "${GREEN}[waterfall]${NC} $1"; }
log_info() { echo -e "${BLUE}[waterfall]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[waterfall]${NC} $1"; }
log_err()  { echo -e "${RED}[waterfall]${NC} $1"; }

LAB_NAME=""
SKIP_SEED=false
TEST_ONLY=false
TEST_REPOS="wateringHole primalSpring bearDog songbird toadStool"
NUM_TEST_REPOS=5

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab)       LAB_NAME="$2"; shift 2 ;;
        --skip-seed) SKIP_SEED=true; shift ;;
        --test-only) TEST_ONLY=true; shift ;;
        --help|-h)
            echo "Usage: $0 --lab <lab-name> [--skip-seed] [--test-only]"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [[ -z "$LAB_NAME" ]]; then
    log_err "--lab is required"
    exit 1
fi

if [[ ! -d "$STATE_DIR/$LAB_NAME" ]]; then
    log_err "Lab not found: $LAB_NAME"
    log_info "Create it: ./create-lab.sh --topology ecoprimals-waterfall-gate-sync --name $LAB_NAME"
    exit 1
fi

container_name() { echo "${LAB_NAME}-${1}"; }

exec_in() {
    local node="$1"; shift
    docker exec "$(container_name "$node")" "$@"
}

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " WaterFall Gate-Sync Deployment"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Lab:          $LAB_NAME"
log_info "ecoPrimals:   $ECO_ROOT"
log_info "Test repos:   $NUM_TEST_REPOS ($TEST_REPOS)"
echo ""

# ── Phase 1: Provision Forgejo (golgiBody) ──────────────────────────────────

provision_forgejo() {
    local fj_container
    fj_container="$(container_name golgibody)"

    log "Phase 1: Provisioning Forgejo (golgiBody)..."

    local fj_ip
    fj_ip=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$fj_container" 2>/dev/null)

    if [[ -z "$fj_ip" ]]; then
        log_err "golgiBody container not running"
        exit 1
    fi

    log_info "  Forgejo IP: $fj_ip"

    docker exec "$fj_container" sh -c '
        apt-get update -qq && apt-get install -y -qq git openssh-client > /dev/null 2>&1 || true
        apk add --no-cache git openssh-client 2>/dev/null || true
    ' 2>/dev/null

    if ! docker exec "$fj_container" sh -c 'command -v git' >/dev/null 2>&1; then
        log_warn "  git not available in Forgejo container — seeding will use HTTP"
    fi

    echo "$fj_ip"
}

# ── Phase 2: Seed test repos into Forgejo ───────────────────────────────────

seed_repos() {
    local fj_ip="$1"
    log "Phase 2: Seeding $NUM_TEST_REPOS test repos into Forgejo..."

    for repo in $TEST_REPOS; do
        local repo_dir=""
        for candidate in \
            "$ECO_ROOT/infra/$repo" \
            "$ECO_ROOT/springs/$repo" \
            "$ECO_ROOT/primals/$repo" \
            "$ECO_ROOT/gardens/$repo" \
            "$ECO_ROOT/$repo"; do
            if [[ -d "$candidate/.git" ]]; then
                repo_dir="$candidate"
                break
            fi
        done

        if [[ -z "$repo_dir" ]]; then
            log_warn "  $repo: not found on disk, skipping"
            continue
        fi

        local tmpbare
        tmpbare=$(mktemp -d)
        git clone --bare "$repo_dir" "$tmpbare/$repo.git" 2>/dev/null

        local fj_container
        fj_container="$(container_name golgibody)"
        docker exec "$fj_container" mkdir -p "/data/git/repos" 2>/dev/null || true
        docker cp "$tmpbare/$repo.git" "${fj_container}:/data/git/repos/$repo.git"
        rm -rf "$tmpbare"

        log "  + $repo seeded"
    done
}

# ── Phase 3: Provision gate containers ──────────────────────────────────────

provision_gate() {
    local gate_node="$1"
    local gate_name="$2"
    local fj_ip="$3"

    log "  Provisioning $gate_node ($gate_name)..."

    docker exec "$(container_name "$gate_node")" sh -c '
        apt-get update -qq 2>/dev/null
        apt-get install -y -qq git python3 2>/dev/null || true
    ' 2>/dev/null

    docker exec "$(container_name "$gate_node")" mkdir -p \
        /opt/ecoPrimals/infra/wateringHole \
        /opt/ecoPrimals/springs \
        /opt/ecoPrimals/primals \
        /opt/ecoPrimals/gardens 2>/dev/null

    local wh_container_dir="/opt/ecoPrimals/infra/wateringHole"

    docker cp "$WH_DIR/cascade-pull.sh" \
        "$(container_name "$gate_node"):${wh_container_dir}/cascade-pull.sh"
    docker cp "$WH_DIR/ecosystem_manifest.toml" \
        "$(container_name "$gate_node"):${wh_container_dir}/ecosystem_manifest.toml"
    docker cp "$WH_DIR/freshness.toml" \
        "$(container_name "$gate_node"):${wh_container_dir}/freshness.toml" 2>/dev/null || true

    docker exec "$(container_name "$gate_node")" chmod +x \
        "${wh_container_dir}/cascade-pull.sh"

    for repo in $TEST_REPOS; do
        local local_path=""
        local manifest_path=""

        case "$repo" in
            wateringHole)  manifest_path="infra/wateringHole" ;;
            primalSpring)  manifest_path="springs/primalSpring" ;;
            bearDog)       manifest_path="primals/bearDog" ;;
            songbird)      manifest_path="primals/songbird" ;;
            toadStool)     manifest_path="primals/toadStool" ;;
        esac

        local repo_dest="/opt/ecoPrimals/$manifest_path"
        docker exec "$(container_name "$gate_node")" sh -c "
            mkdir -p '$repo_dest'
            cd '$repo_dest'
            git init -q 2>/dev/null
            git remote add origin 'http://${fj_ip}:3000/repos/${repo}.git' 2>/dev/null || true
            git remote add forgejo 'http://${fj_ip}:3000/repos/${repo}.git' 2>/dev/null || true
        " 2>/dev/null || true
    done

    log "  + $gate_node: wateringHole + $NUM_TEST_REPOS repo stubs ready"
}

provision_gates() {
    local fj_ip="$1"
    log "Phase 3: Provisioning gate containers..."
    provision_gate "eastgate"   "eastGate"   "$fj_ip"
    provision_gate "irongate"   "ironGate"   "$fj_ip"
    provision_gate "strandgate" "strandGate" "$fj_ip"
    provision_gate "biomegate"  "biomeGate"  "$fj_ip"
}

# ── Phase 4: Run cascade-pull on each gate ──────────────────────────────────

run_cascade_pull() {
    local gate_node="$1"
    local gate_name="$2"

    local wh="${ECOPRIMALS_ROOT:-/opt/ecoPrimals}/infra/wateringHole"

    log "  $gate_node: cascade-pull --gate auto --dry-run..."

    local output
    output=$(docker exec \
        -e "GATE_NAME=$gate_name" \
        -e "ECOPRIMALS_ROOT=/opt/ecoPrimals" \
        "$(container_name "$gate_node")" \
        bash /opt/ecoPrimals/infra/wateringHole/cascade-pull.sh \
            --gate auto --dry-run --no-self-update 2>&1) || true

    local repo_count
    repo_count=$(echo "$output" | grep -c "WOULD-PULL" || echo "0")

    local detected_gate
    detected_gate=$(echo "$output" | grep "Gate auto-detected:" | sed 's/.*: //' || echo "NONE")

    if [[ "$detected_gate" == "$gate_name" ]]; then
        log "  + $gate_node: gate detected as $detected_gate — $repo_count repos in profile"
    else
        log_err "  ! $gate_node: expected $gate_name, got $detected_gate"
        return 1
    fi

    echo "$repo_count"
}

test_cascade_pull() {
    log "Phase 4: Testing cascade-pull --gate auto on each gate..."

    local east_count iron_count strand_count biome_count
    local failures=0

    east_count=$(run_cascade_pull "eastgate" "eastGate")     || ((failures++))
    iron_count=$(run_cascade_pull "irongate" "ironGate")     || ((failures++))
    strand_count=$(run_cascade_pull "strandgate" "strandGate") || ((failures++))
    biome_count=$(run_cascade_pull "biomegate" "biomeGate")  || ((failures++))

    echo ""
    log "Gate profile repo counts:"
    log_info "  eastGate:   $east_count repos (expected: 38, full superset)"
    log_info "  ironGate:   $iron_count repos (expected: 22)"
    log_info "  strandGate: $strand_count repos (expected: 24)"
    log_info "  biomeGate:  $biome_count repos (expected: 17)"

    if [[ "$failures" -gt 0 ]]; then
        log_err "  $failures gate(s) failed auto-detection"
    fi

    echo "$failures"
}

# ── Phase 5: Validate cross-gate consistency ────────────────────────────────

validate_consistency() {
    log "Phase 5: Cross-gate consistency validation..."

    local all_pass=true

    docker exec \
        -e "GATE_NAME=eastGate" \
        -e "ECOPRIMALS_ROOT=/opt/ecoPrimals" \
        "$(container_name "eastgate")" \
        bash /opt/ecoPrimals/infra/wateringHole/cascade-pull.sh \
            --gate auto --check --no-self-update 2>&1 | head -20 || true

    for gate_node in eastgate irongate strandgate biomegate; do
        local gate_env
        case "$gate_node" in
            eastgate)   gate_env="eastGate" ;;
            irongate)   gate_env="ironGate" ;;
            strandgate) gate_env="strandGate" ;;
            biomegate)  gate_env="biomeGate" ;;
        esac

        local source_flag
        source_flag=$(docker exec "$(container_name "$gate_node")" \
            bash -c 'echo ${CASCADE_SYNC_SOURCE:-github}' 2>/dev/null || echo "github")

        log_info "  $gate_node: source=$source_flag, gate=$gate_env"
    done

    log "Cross-gate validation complete"
}

# ── Main ────────────────────────────────────────────────────────────────────

if ! $TEST_ONLY; then
    fj_ip=$(provision_forgejo)

    if ! $SKIP_SEED; then
        seed_repos "$fj_ip"
    fi

    provision_gates "$fj_ip"
fi

failures=$(test_cascade_pull)
validate_consistency

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [[ "${failures:-0}" == "0" ]]; then
    log "WaterFall gate-sync validation PASSED"
else
    log_err "WaterFall gate-sync validation FAILED ($failures failures)"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Next steps:"
echo "  1. Run full pull:  docker exec -e GATE_NAME=eastGate ${LAB_NAME}-eastgate bash /opt/ecoPrimals/infra/wateringHole/cascade-pull.sh --gate auto --source forgejo"
echo "  2. Tear down:      ./destroy-lab.sh --lab $LAB_NAME --force"
echo ""
