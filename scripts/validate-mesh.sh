#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# validate-mesh.sh — Post-deploy mesh validation for libvirt topologies
#
# Orchestrates validation after VMs boot:
#   1. Wait for cloud-init complete on all VMs
#   2. SSH into each, run `membrane gate.status`
#   3. Verify songBird federation mesh (3-way peering)
#   4. Verify bearDog crypto spine on all nodes
#   5. Run temporal.cascade on primary node
#   6. Validate zero version skew
#
# Usage:
#   ./validate-mesh.sh --topology irongate-nucleus-mesh [--timeout 600]

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"

TOPOLOGY=""
TIMEOUT=600
SSH_KEY="${HOME}/.ssh/id_ed25519"
SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=10 -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"

usage() {
    cat << EOF
Usage: $0 --topology <name> [options]

Validate a libvirt mesh topology after VM boot.

Required:
    --topology <name>     Topology name (must exist in topologies/)

Optional:
    --timeout <secs>      Max wait for cloud-init (default: 600)
    --ssh-key <path>      SSH private key path (default: ~/.ssh/id_ed25519)
    --help                Show this help

Examples:
    $0 --topology irongate-nucleus-mesh
    $0 --topology irongate-nucleus-mesh --timeout 900
EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --topology) TOPOLOGY="$2"; shift 2 ;;
        --timeout)  TIMEOUT="$2"; shift 2 ;;
        --ssh-key)  SSH_KEY="$2"; shift 2 ;;
        --help)     usage ;;
        *)          echo -e "${RED}Unknown option: $1${NC}"; usage ;;
    esac
done

[ -z "$TOPOLOGY" ] && { echo -e "${RED}--topology required${NC}"; usage; }

TOPOLOGY_FILE="$BENCHSCALE_ROOT/topologies/${TOPOLOGY}.yaml"
[ -f "$TOPOLOGY_FILE" ] || { echo -e "${RED}Topology not found: $TOPOLOGY_FILE${NC}"; exit 1; }

log()      { echo -e "${GREEN}[mesh]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[mesh]${NC} $1"; }
log_err()  { echo -e "${RED}[mesh]${NC} $1"; }
log_info() { echo -e "${BLUE}[mesh]${NC} $1"; }

get_node_ips() {
    grep -E '^\s+ip:' "$TOPOLOGY_FILE" | sed 's/.*ip:\s*//' | tr -d '"' | tr -d "'"
}

get_node_names() {
    grep -E '^\s+-\s+name:' "$TOPOLOGY_FILE" | sed 's/.*name:\s*//' | tr -d '"' | tr -d "'"
}

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " benchScale Mesh Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log_info "Topology: $TOPOLOGY"
echo ""

NODES=()
IPS=()
while IFS= read -r name; do [ -n "$name" ] && NODES+=("$name"); done < <(get_node_names)
while IFS= read -r ip; do [ -n "$ip" ] && IPS+=("$ip"); done < <(get_node_ips)

if [ ${#NODES[@]} -ne ${#IPS[@]} ]; then
    log_err "Node/IP count mismatch: ${#NODES[@]} nodes, ${#IPS[@]} IPs"
    exit 1
fi

log "Found ${#NODES[@]} nodes: ${NODES[*]}"

# Step 1: Wait for cloud-init
log "Step 1: Waiting for cloud-init on all VMs (timeout: ${TIMEOUT}s)..."
DEADLINE=$((SECONDS + TIMEOUT))
for i in "${!NODES[@]}"; do
    node="${NODES[$i]}"
    ip="${IPS[$i]}"
    log_info "  Waiting for $node ($ip)..."
    while [ $SECONDS -lt $DEADLINE ]; do
        if ssh $SSH_OPTS -i "$SSH_KEY" "irongate@${ip}" "test -f /var/lib/cloud/instance/boot-finished" 2>/dev/null; then
            log "  ✓ $node cloud-init complete"
            break
        fi
        sleep 5
    done
done

# Step 2: gate.status on each
log "Step 2: Running gate.status on each VM..."
PASS=0
FAIL=0
for i in "${!NODES[@]}"; do
    node="${NODES[$i]}"
    ip="${IPS[$i]}"
    RESULT=$(ssh $SSH_OPTS -i "$SSH_KEY" "irongate@${ip}" "membrane gate.status 2>&1" 2>/dev/null || echo "UNREACHABLE")
    if echo "$RESULT" | grep -qi "healthy\|active\|ready"; then
        log "  ✓ $node: healthy"
        PASS=$((PASS + 1))
    else
        log_warn "  ✗ $node: $(echo "$RESULT" | head -1)"
        FAIL=$((FAIL + 1))
    fi
done

# Step 3: Federation mesh
log "Step 3: Verifying songBird federation mesh..."
for i in "${!NODES[@]}"; do
    node="${NODES[$i]}"
    ip="${IPS[$i]}"
    FED=$(ssh $SSH_OPTS -i "$SSH_KEY" "irongate@${ip}" \
        "echo '{\"jsonrpc\":\"2.0\",\"method\":\"federation.status\",\"id\":1}' | socat -t2 - UNIX-CONNECT:/run/membrane/songbird.sock 2>/dev/null" 2>/dev/null || echo "{}")
    ENABLED=$(echo "$FED" | jq -r '.result.enabled // "unknown"' 2>/dev/null)
    CONNS=$(echo "$FED" | jq -r '.result.active_connections // 0' 2>/dev/null)
    if [ "$ENABLED" = "true" ] && [ "$CONNS" -gt 0 ]; then
        log "  ✓ $node: federation enabled, $CONNS active connections"
    elif [ "$ENABLED" = "true" ]; then
        log_warn "  △ $node: federation enabled, 0 connections (peers may still be joining)"
    else
        log_err "  ✗ $node: federation=$ENABLED"
    fi
done

# Step 4: bearDog crypto spine
log "Step 4: Verifying bearDog crypto spine..."
for i in "${!NODES[@]}"; do
    node="${NODES[$i]}"
    ip="${IPS[$i]}"
    HEALTH=$(ssh $SSH_OPTS -i "$SSH_KEY" "irongate@${ip}" \
        "echo '{\"jsonrpc\":\"2.0\",\"method\":\"health\",\"id\":1}' | socat -t2 - UNIX-CONNECT:/run/membrane/beardog.sock 2>/dev/null" 2>/dev/null || echo "{}")
    if echo "$HEALTH" | grep -q "result"; then
        log "  ✓ $node: bearDog alive"
    else
        log_err "  ✗ $node: bearDog unreachable"
    fi
done

# Step 5: Cascade on primary
log "Step 5: Running temporal.cascade on ironGate..."
PRIMARY_IP="${IPS[0]}"
CASCADE=$(ssh $SSH_OPTS -i "$SSH_KEY" "irongate@${PRIMARY_IP}" \
    "membrane temporal.cascade --dry-run 2>&1" 2>/dev/null || echo "FAILED")
if echo "$CASCADE" | grep -qi "up.to.date\|no changes\|sync complete"; then
    log "  ✓ Cascade: no version skew"
else
    log_warn "  △ Cascade output: $(echo "$CASCADE" | tail -3)"
fi

# Step 6: Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "Mesh Validation Complete"
log_info "  Nodes: ${#NODES[@]}"
log_info "  Passed gate.status: $PASS"
log_info "  Failed gate.status: $FAIL"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

[ $FAIL -eq 0 ] && exit 0 || exit 1
