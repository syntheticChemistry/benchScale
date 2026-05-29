#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# deploy-vps-depot.sh — Validate VPS depot deployment pipeline in benchScale
#
# Validates the Common NUCLEUS Deployment pattern: VPS depot (golgiBody)
# deploys to 6 gates representing real hardware classes:
#   - LAN cluster: eastGate (full), ironGate (full)
#   - WAN household: flockGate (node_atomic, BEHIND_NAT, TURN relay)
#   - Roaming mobile: swiftGate (full, LAN->WAN transition)
#   - Cold NAS: westGate (fieldMouse, UDS-only)
#   - Family friend: kinGate (tower-only, BEHIND_NAT, TURN relay)
#
# 8 validation phases:
#   1. Provision VPS depot (binaries + family seed + Songbird + RustDesk)
#   2. Deploy LAN gates (full --systemd)
#   3. Deploy NAS gate (fieldMouse --systemd --uds-only)
#   4. Deploy WAN flockGate (node --systemd, TURN relay)
#   5. Deploy friend kinGate (tower --systemd, TURN relay)
#   6. Deploy roaming swiftGate (full, LAN then degrade to WAN)
#   7. Federation mesh validation (peers, capability routing, TURN)
#   8. Cascade-pull + health sweep
#
# Usage:
#   ./deploy-vps-depot.sh --lab <lab-name>
#   ./deploy-vps-depot.sh --lab depot-test --phase 7    # run single phase
#   ./deploy-vps-depot.sh --lab depot-test --dry-run    # show plan
#
# Prerequisites:
#   - Lab created: create-lab.sh --topology ecoprimals-vps-depot-deploy
#   - Docker running
#   - plasmidBin binaries built (or will use --mode pull)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"

ECO_ROOT="${ECOPRIMALS_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
PLASMIDBIN="$ECO_ROOT/infra/plasmidBin"
WH_DIR="$ECO_ROOT/infra/wateringHole"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

log()      { echo -e "${GREEN}[vps-depot]${NC} $1"; }
log_info() { echo -e "${CYAN}[vps-depot]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[vps-depot]${NC} $1"; }
log_err()  { echo -e "${RED}[vps-depot]${NC} $1"; }

LAB_NAME=""
DRY_RUN=false
PHASE=""
FAMILY_ID="e8b62b6e"
PASS=0
FAIL=0
SKIP=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab)     LAB_NAME="$2"; shift 2 ;;
        --phase)   PHASE="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        --help|-h)
            echo "Usage: $0 --lab <lab-name> [--phase N] [--dry-run]"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [[ -z "$LAB_NAME" ]]; then
    echo "ERROR: --lab required"
    exit 1
fi

LAB_STATE="$STATE_DIR/$LAB_NAME"
if [[ ! -d "$LAB_STATE" ]]; then
    echo "ERROR: Lab '$LAB_NAME' not found. Create it first:"
    echo "  ./create-lab.sh --topology ecoprimals-vps-depot-deploy --name $LAB_NAME --hypervisor docker"
    exit 1
fi

docker_exec() {
    local container="$1"; shift
    docker exec "${LAB_NAME}-${container}" "$@"
}

should_run() {
    [[ -z "$PHASE" || "$PHASE" == "$1" ]]
}

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [[ "$expected" == "$actual" ]]; then
        log "  PASS: $label (got: $actual)"
        PASS=$((PASS + 1))
    else
        log_err "  FAIL: $label (expected: $expected, got: $actual)"
        FAIL=$((FAIL + 1))
    fi
}

assert_contains() {
    local label="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        log "  PASS: $label"
        PASS=$((PASS + 1))
    else
        log_err "  FAIL: $label (missing: $needle)"
        FAIL=$((FAIL + 1))
    fi
}

assert_ge() {
    local label="$1" min="$2" actual="$3"
    if [[ "$actual" -ge "$min" ]]; then
        log "  PASS: $label (got: $actual >= $min)"
        PASS=$((PASS + 1))
    else
        log_err "  FAIL: $label (got: $actual, expected >= $min)"
        FAIL=$((FAIL + 1))
    fi
}

echo ""
log_info "════════════════════════════════════════════════════"
log_info "  VPS Depot Deployment Validation"
log_info "════════════════════════════════════════════════════"
echo ""
log_info "Lab:       $LAB_NAME"
log_info "Family:    $FAMILY_ID"
log_info "Phases:    ${PHASE:-all}"
log_info "Dry run:   $DRY_RUN"
echo ""

# ── Phase 1: Provision VPS depot ─────────────────────────────────────────

if should_run 1; then
    log_info "=== Phase 1: Provision VPS depot (golgiBody) ==="

    if $DRY_RUN; then
        log "  [dry-run] Would install plasmidBin tools in golgibody container"
        log "  [dry-run] Would generate family seed"
        log "  [dry-run] Would start Songbird TURN + federation hub"
    else
        log "  Installing plasmidBin tooling..."
        docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-golgibody:/opt/plasmidBin/ports.env"
        docker cp "$PLASMIDBIN/nucleus_launcher.sh" "${LAB_NAME}-golgibody:/opt/plasmidBin/nucleus_launcher.sh"
        docker cp "$PLASMIDBIN/start_primal.sh" "${LAB_NAME}-golgibody:/opt/plasmidBin/start_primal.sh"
        docker cp "$PLASMIDBIN/deploy_gate.sh" "${LAB_NAME}-golgibody:/opt/plasmidBin/deploy_gate.sh"
        docker cp "$PLASMIDBIN/fetch.sh" "${LAB_NAME}-golgibody:/opt/plasmidBin/fetch.sh"

        log "  Generating family seed..."
        docker_exec golgibody bash -c "
            mkdir -p /opt/membrane /run/membrane /etc/membrane/family/nodes
            echo '$FAMILY_ID' > /etc/membrane/family/family_id
            head -c 32 /dev/urandom | xxd -p | tr -d '\n' > /etc/membrane/family/.beacon.seed
            chmod 600 /etc/membrane/family/.beacon.seed
            cat /etc/membrane/family/.beacon.seed > /opt/membrane/tower.env.seed
        " 2>/dev/null || true

        log "  Generating per-gate lineage seeds..."
        for gate in eastGate ironGate flockGate swiftGate westGate kinGate; do
            docker_exec golgibody bash -c "
                head -c 32 /dev/urandom | xxd -p | tr -d '\n' > /etc/membrane/family/nodes/${gate}.lineage.seed
                chmod 600 /etc/membrane/family/nodes/${gate}.lineage.seed
            " 2>/dev/null || true
        done

        log "  Writing tower.env..."
        SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "fallback-seed")
        docker_exec golgibody bash -c "cat > /opt/membrane/tower.env << EOF
MEMBRANE_ROLE=depot
MEMBRANE_GATE_ID=golgiBody
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=golgiBody
BEARDOG_NODE_ID=golgiBody
SONGBIRD_NODE_ID=golgiBody
EOF
chmod 600 /opt/membrane/tower.env" 2>/dev/null

        SEED_LEN=${#SEED}
        assert_ge "Family seed generated" 32 "$SEED_LEN"

        LINEAGE_COUNT=$(docker_exec golgibody ls /etc/membrane/family/nodes/ 2>/dev/null | wc -l)
        assert_eq "Lineage seeds for 6 gates" "6" "$LINEAGE_COUNT"
    fi

    echo ""
fi

# ── Phase 2: Deploy LAN gates (full NUCLEUS, systemd) ───────────────────

if should_run 2; then
    log_info "=== Phase 2: Deploy LAN gates (eastGate, ironGate) ==="

    for gate in eastgate irongate; do
        NODE_ID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "$gate")
        COMP=$(docker_exec "$gate" printenv COMPOSITION 2>/dev/null || echo "full")

        log "  Deploying $gate ($NODE_ID) — composition: $COMP"

        if $DRY_RUN; then
            log "  [dry-run] deploy_gate.sh $gate --composition $COMP --systemd"
        else
            docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-${gate}:/opt/plasmidBin/ports.env"
            docker cp "$PLASMIDBIN/deploy_gate.sh" "${LAB_NAME}-${gate}:/opt/plasmidBin/"

            SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "")
            docker_exec "$gate" bash -c "
                mkdir -p /opt/membrane /run/membrane
                cat > /opt/membrane/tower.env << EOF
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=$NODE_ID
BEARDOG_NODE_ID=$NODE_ID
SONGBIRD_NODE_ID=$NODE_ID
SONGBIRD_PEERS=golgiBody@golgibody:7700
EOF
                chmod 600 /opt/membrane/tower.env
            " 2>/dev/null

            EXPECTED=13
            assert_eq "$gate tower.env NODE_ID" "$NODE_ID" "$(docker_exec "$gate" bash -c 'grep NODE_ID /opt/membrane/tower.env | head -1 | cut -d= -f2' 2>/dev/null)"
            log "  $gate: tower.env written, composition=$COMP, expected $EXPECTED primals"
        fi
    done

    echo ""
fi

# ── Phase 3: Deploy NAS gate (westGate, fieldMouse, UDS-only) ────────────

if should_run 3; then
    log_info "=== Phase 3: Deploy NAS gate (westGate — fieldMouse) ==="

    if $DRY_RUN; then
        log "  [dry-run] deploy_gate.sh westgate --composition fieldMouse --systemd --uds-only"
    else
        SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "")

        docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-westgate:/opt/plasmidBin/ports.env"
        docker_exec westgate bash -c "
            mkdir -p /opt/membrane /run/membrane
            cat > /opt/membrane/tower.env << EOF
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=westGate
BEARDOG_NODE_ID=westGate
SONGBIRD_NODE_ID=westGate
SONGBIRD_PEERS=golgiBody@golgibody:7700
EOF
            chmod 600 /opt/membrane/tower.env
        " 2>/dev/null

        FM_PRIMALS=$(docker_exec westgate bash -c "source /opt/plasmidBin/ports.env 2>/dev/null && primals_for_composition fieldMouse" 2>/dev/null || echo "")
        FM_COUNT=$(echo "$FM_PRIMALS" | wc -w)
        assert_eq "fieldMouse composition = 7 primals" "7" "$FM_COUNT"

        UDS_FLAG=$(docker_exec westgate printenv UDS_ONLY 2>/dev/null || echo "")
        assert_eq "westGate UDS_ONLY env set" "true" "$UDS_FLAG"
    fi

    echo ""
fi

# ── Phase 4: Deploy WAN flockGate (node, BEHIND_NAT, TURN) ──────────────

if should_run 4; then
    log_info "=== Phase 4: Deploy WAN flockGate (node_atomic, TURN relay) ==="

    if $DRY_RUN; then
        log "  [dry-run] deploy_gate.sh flockgate --composition node --systemd --node-id flockGate"
    else
        SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "")

        docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-flockgate:/opt/plasmidBin/ports.env"
        docker_exec flockgate bash -c "
            mkdir -p /opt/membrane /run/membrane
            cat > /opt/membrane/tower.env << EOF
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=flockGate
BEARDOG_NODE_ID=flockGate
SONGBIRD_NODE_ID=flockGate
SONGBIRD_PEERS=golgiBody@golgibody:7700
SONGBIRD_TURN_SERVER=golgibody:3478
SONGBIRD_TURN_USERNAME=nucleus-relay
EOF
            chmod 600 /opt/membrane/tower.env
        " 2>/dev/null

        NAT_FLAG=$(docker_exec flockgate printenv BEHIND_NAT 2>/dev/null || echo "")
        assert_eq "flockGate BEHIND_NAT" "true" "$NAT_FLAG"

        NODE_PRIMALS=$(docker_exec flockgate bash -c "source /opt/plasmidBin/ports.env 2>/dev/null && primals_for_composition node" 2>/dev/null || echo "")
        NODE_COUNT=$(echo "$NODE_PRIMALS" | wc -w)
        assert_eq "node composition = 6 primals" "6" "$NODE_COUNT"

        TURN_CFG=$(docker_exec flockgate bash -c "grep SONGBIRD_TURN_SERVER /opt/membrane/tower.env" 2>/dev/null || echo "")
        assert_contains "flockGate TURN server configured" "golgibody:3478" "$TURN_CFG"
    fi

    echo ""
fi

# ── Phase 5: Deploy friend kinGate (tower-only, TURN relay) ─────────────

if should_run 5; then
    log_info "=== Phase 5: Deploy friend tower (kinGate — tower only) ==="

    if $DRY_RUN; then
        log "  [dry-run] deploy_gate.sh kingate --composition tower --systemd --node-id kinGate"
    else
        SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "")

        docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-kingate:/opt/plasmidBin/ports.env"
        docker_exec kingate bash -c "
            mkdir -p /opt/membrane /run/membrane
            cat > /opt/membrane/tower.env << EOF
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=kinGate
BEARDOG_NODE_ID=kinGate
SONGBIRD_NODE_ID=kinGate
SONGBIRD_PEERS=golgiBody@golgibody:7700
SONGBIRD_TURN_SERVER=golgibody:3478
SONGBIRD_TURN_USERNAME=nucleus-relay
EOF
            chmod 600 /opt/membrane/tower.env
        " 2>/dev/null

        TOWER_PRIMALS=$(docker_exec kingate bash -c "source /opt/plasmidBin/ports.env 2>/dev/null && primals_for_composition tower" 2>/dev/null || echo "")
        TOWER_COUNT=$(echo "$TOWER_PRIMALS" | wc -w)
        assert_eq "tower composition = 3 primals" "3" "$TOWER_COUNT"

        NAT_FLAG=$(docker_exec kingate printenv BEHIND_NAT 2>/dev/null || echo "")
        assert_eq "kinGate BEHIND_NAT" "true" "$NAT_FLAG"
    fi

    echo ""
fi

# ── Phase 6: Deploy roaming swiftGate (full, LAN->WAN transition) ───────

if should_run 6; then
    log_info "=== Phase 6: Deploy roaming swiftGate (LAN start, WAN degrade) ==="

    if $DRY_RUN; then
        log "  [dry-run] deploy full --systemd to swiftgate"
        log "  [dry-run] tc netem add 100ms latency + 2% loss mid-run"
    else
        SEED=$(docker_exec golgibody cat /etc/membrane/family/.beacon.seed 2>/dev/null || echo "")

        docker cp "$PLASMIDBIN/ports.env" "${LAB_NAME}-swiftgate:/opt/plasmidBin/ports.env"
        docker_exec swiftgate bash -c "
            mkdir -p /opt/membrane /run/membrane
            cat > /opt/membrane/tower.env << EOF
FAMILY_ID=$FAMILY_ID
BEARDOG_FAMILY_SEED=$SEED
FAMILY_SEED=$SEED
NODE_ID=swiftGate
BEARDOG_NODE_ID=swiftGate
SONGBIRD_NODE_ID=swiftGate
SONGBIRD_PEERS=golgiBody@golgibody:7700
SONGBIRD_TURN_SERVER=golgibody:3478
SONGBIRD_TURN_USERNAME=nucleus-relay
EOF
            chmod 600 /opt/membrane/tower.env
        " 2>/dev/null

        ROAMING=$(docker_exec swiftgate printenv ROAMING 2>/dev/null || echo "")
        assert_eq "swiftGate ROAMING flag" "true" "$ROAMING"

        log "  Verifying LAN conditions (pre-transition)..."
        LAN_LATENCY=$(docker_exec swiftgate bash -c "tc qdisc show 2>/dev/null | grep -o 'delay [0-9.]*ms' | head -1" 2>/dev/null || echo "not configured")
        log "  Current network: $LAN_LATENCY"

        log "  Degrading swiftGate to WAN conditions (mobile_cell)..."
        docker_exec swiftgate bash -c "
            tc qdisc replace dev eth0 root netem delay 100ms 20ms loss 2% rate 10mbit 2>/dev/null || echo 'tc not available'
        " 2>/dev/null || log_warn "  tc netem not available (needs NET_ADMIN)"

        WAN_LATENCY=$(docker_exec swiftgate bash -c "tc qdisc show 2>/dev/null | grep -o 'delay [0-9.]*ms' | head -1" 2>/dev/null || echo "transition attempted")
        log "  Post-transition network: $WAN_LATENCY"
    fi

    echo ""
fi

# ── Phase 7: Federation mesh validation ─────────────────────────────────

if should_run 7; then
    log_info "=== Phase 7: Federation mesh validation ==="

    if $DRY_RUN; then
        log "  [dry-run] Would verify discovery.peers on all gates"
        log "  [dry-run] Would verify ipc.resolve cross-gate routing"
        log "  [dry-run] Would verify TURN relay for NAT'd gates"
    else
        log "  Checking connectivity: all gates -> golgiBody..."
        for gate in eastgate irongate flockgate swiftgate westgate kingate; do
            PING=$(docker_exec "$gate" bash -c "ping -c 1 -W 2 golgibody 2>/dev/null | grep -o '[0-9.]* ms' | head -1" 2>/dev/null || echo "unreachable")
            NODE_ID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "$gate")
            log "  $NODE_ID -> golgiBody: $PING"
        done

        log ""
        log "  Checking TURN relay path for NAT'd gates..."
        for gate in flockgate kingate; do
            NODE_ID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "$gate")
            TURN=$(docker_exec "$gate" bash -c "grep SONGBIRD_TURN_SERVER /opt/membrane/tower.env 2>/dev/null | cut -d= -f2" 2>/dev/null || echo "not set")
            assert_contains "$NODE_ID has TURN config" "golgibody:3478" "$TURN"
        done

        log ""
        log "  Checking shared family identity across all nodes..."
        for gate in eastgate irongate flockgate swiftgate westgate kingate; do
            NODE_ID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "$gate")
            FID=$(docker_exec "$gate" bash -c "grep '^FAMILY_ID=' /opt/membrane/tower.env 2>/dev/null | cut -d= -f2" 2>/dev/null || echo "")
            assert_eq "$NODE_ID FAMILY_ID matches" "$FAMILY_ID" "$FID"
        done

        log ""
        log "  Checking composition sizes..."
        COMPOSITIONS=(
            "eastgate:full:13"
            "irongate:full:13"
            "flockgate:node:6"
            "swiftgate:full:13"
            "westgate:fieldMouse:7"
            "kingate:tower:3"
        )
        for entry in "${COMPOSITIONS[@]}"; do
            IFS=: read -r gate comp expected <<< "$entry"
            ACTUAL=$(docker_exec "$gate" bash -c "source /opt/plasmidBin/ports.env 2>/dev/null && primals_for_composition $comp | wc -w" 2>/dev/null || echo "0")
            NODE_ID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "$gate")
            assert_eq "$NODE_ID composition $comp" "$expected" "$ACTUAL"
        done
    fi

    echo ""
fi

# ── Phase 8: Cascade-pull + health sweep ─────────────────────────────────

if should_run 8; then
    log_info "=== Phase 8: Cascade-pull + health sweep ==="

    if $DRY_RUN; then
        log "  [dry-run] Would test cascade-pull --gate golgiBody on VPS"
        log "  [dry-run] Would run nucleus_launcher --seed-only on each gate"
    else
        log "  Verifying golgiBody cascade-pull profile exists..."
        docker cp "$WH_DIR/cascade-pull.sh" "${LAB_NAME}-golgibody:/opt/ecoPrimals/infra/wateringHole/cascade-pull.sh" 2>/dev/null || true
        docker cp "$WH_DIR/ecosystem_manifest.toml" "${LAB_NAME}-golgibody:/opt/ecoPrimals/infra/wateringHole/ecosystem_manifest.toml" 2>/dev/null || true

        PROFILE=$(docker_exec golgibody bash -c "
            export ECOPRIMALS_ROOT=/opt/ecoPrimals
            export GATE_NAME=golgiBody
            mkdir -p /opt/ecoPrimals/primals /opt/ecoPrimals/infra/wateringHole
            cd /opt/ecoPrimals/infra/wateringHole
            source cascade-pull.sh 2>/dev/null && primals_for_composition fieldMouse 2>/dev/null | wc -w || echo 0
        " 2>/dev/null || echo "0")
        log "  golgiBody fieldMouse primals: $PROFILE"

        log ""
        log "  Summary of gate identity + composition:"
        echo ""
        printf "  %-14s %-12s %-10s %-8s %-6s\n" "Gate" "NODE_ID" "Comp" "NAT" "TURN"
        printf "  %-14s %-12s %-10s %-8s %-6s\n" "──────────" "──────────" "────────" "──────" "────"
        for gate in golgibody eastgate irongate flockgate swiftgate westgate kingate; do
            NID=$(docker_exec "$gate" printenv NODE_ID 2>/dev/null || echo "?")
            COMP=$(docker_exec "$gate" printenv COMPOSITION 2>/dev/null || echo "?")
            NAT=$(docker_exec "$gate" printenv BEHIND_NAT 2>/dev/null || echo "false")
            TURN=$(docker_exec "$gate" bash -c "grep -c SONGBIRD_TURN_SERVER /opt/membrane/tower.env 2>/dev/null || echo 0" 2>/dev/null | tr -d '[:space:]')
            [[ "${TURN:-0}" -gt 0 ]] 2>/dev/null && TURN_STATUS="yes" || TURN_STATUS="no"
            printf "  %-14s %-12s %-10s %-8s %-6s\n" "$gate" "$NID" "$COMP" "$NAT" "$TURN_STATUS"
        done
    fi

    echo ""
fi

# ── Summary ──────────────────────────────────────────────────────────────

echo ""
log_info "════════════════════════════════════════════════════"
log_info "  VPS Depot Validation Complete"
log_info "════════════════════════════════════════════════════"
echo ""
log_info "  PASS: $PASS"
[[ $FAIL -gt 0 ]] && log_err "  FAIL: $FAIL" || log "  FAIL: 0"
[[ $SKIP -gt 0 ]] && log_warn "  SKIP: $SKIP"
echo ""

TOTAL=$((PASS + FAIL))
if [[ $TOTAL -gt 0 ]]; then
    PCT=$(( (PASS * 100) / TOTAL ))
    log_info "  Score: $PCT% ($PASS / $TOTAL)"
fi

echo ""
log_info "  Next steps:"
log_info "    1. Fix any failures above"
log_info "    2. Run with real primals: deploy-ecoprimals.sh --lab $LAB_NAME"
log_info "    3. Onboard real gates: onboard-gate-relay.sh <gate> --vps-host 157.230.3.183"
echo ""

exit $FAIL
