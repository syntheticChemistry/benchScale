#!/usr/bin/env bash
# Current helper script; canonical teardown is `cargo run -- destroy` / `benchscale destroy` (see scripts/README.md).
#
# benchScale Lab Destruction Script
#
# Tears down a lab environment

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

LAB_NAME=""
FORCE=false

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"

usage() {
    cat << EOF
Usage: $0 --lab <lab-name> [options]

Destroy a benchScale lab environment.

Required Arguments:
    --lab <name>            Lab name to destroy

Optional Arguments:
    --force                 Skip confirmation prompt
    --help                  Show this help message

Examples:
    $0 --lab test-lab-01
    $0 --lab lan-test --force

EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab) LAB_NAME="$2"; shift 2 ;;
        --force) FORCE=true; shift ;;
        --help) usage ;;
        *) echo -e "${RED}Error: Unknown option $1${NC}"; usage ;;
    esac
done

if [ -z "$LAB_NAME" ]; then
    echo -e "${RED}Error: --lab is required${NC}"
    usage
fi

log() { echo -e "${GREEN}[benchScale]${NC} $1"; }
log_info() { echo -e "${BLUE}[benchScale]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[benchScale]${NC} $1"; }
log_error() { echo -e "${RED}[benchScale]${NC} $1"; }

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " benchScale Lab Destruction"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Check if lab exists
if [ ! -d "$STATE_DIR/$LAB_NAME" ]; then
    log_error "Lab not found: $LAB_NAME"
    exit 1
fi

# Get lab info
TOPOLOGY=$(grep "^topology:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')
HYPERVISOR=$(grep "^hypervisor:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')

log_warn "Lab:        $LAB_NAME"
log_warn "Topology:   $TOPOLOGY"
log_warn "Hypervisor: $HYPERVISOR"
echo ""

# Confirmation
if [ "$FORCE" != "true" ]; then
    log_warn "This will permanently destroy the lab and all its data."
    read -p "Are you sure? (yes/no): " -r
    echo
    if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
        log "Aborted."
        exit 0
    fi
fi

# Destroy nodes
log "Destroying nodes..."

NODES_FILE="$STATE_DIR/$LAB_NAME/nodes.txt"
NETWORK_NAME=$(grep "^network:" "$STATE_DIR/$LAB_NAME/info.yaml" 2>/dev/null | awk '{print $2}')

case "$HYPERVISOR" in
    docker)
        if [ -f "$NODES_FILE" ]; then
            while IFS= read -r container; do
                [ -z "$container" ] && continue
                log "Removing $container..."
                docker rm -f "$container" 2>/dev/null || log_warn "Could not remove $container"
            done < "$NODES_FILE"
        else
            CONTAINERS=$(docker ps -a --filter "name=^${LAB_NAME}-" --format '{{.Names}}' 2>/dev/null || true)
            if [ -n "$CONTAINERS" ]; then
                while IFS= read -r container; do
                    log "Removing $container..."
                    docker rm -f "$container" 2>/dev/null || log_warn "Could not remove $container"
                done <<< "$CONTAINERS"
            else
                log_warn "No containers found for lab: $LAB_NAME"
            fi
        fi

        if [ -n "$NETWORK_NAME" ]; then
            log "Removing network: $NETWORK_NAME"
            docker network rm "$NETWORK_NAME" 2>/dev/null || log_warn "Could not remove network"
        fi
        ;;

    lxd)
        CONTAINERS=$(lxc list --format csv -c n 2>/dev/null | grep "^${LAB_NAME}-" || true)
        if [ -n "$CONTAINERS" ]; then
            while IFS= read -r container; do
                log "Stopping $container..."
                lxc stop "$container" --force 2>/dev/null || log_warn "Could not stop $container"
                log "Deleting $container..."
                lxc delete "$container" 2>/dev/null || log_warn "Could not delete $container"
            done <<< "$CONTAINERS"
        else
            log_warn "No containers found for lab: $LAB_NAME"
        fi
        ;;

    *)
        log_warn "Unsupported hypervisor for automated destroy: $HYPERVISOR"
        log_warn "Manual cleanup may be required"
        ;;
esac

# Remove lab state
log "Removing lab state..."
rm -rf "$STATE_DIR/$LAB_NAME"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "✅ Lab destroyed successfully!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

