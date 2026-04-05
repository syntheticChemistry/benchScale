#!/usr/bin/env bash
# Archived: legacy LXC primal deployment (historical filename — not an ISO downloader).
# Prefer deploy-ecoprimals.sh and the benchscale CLI for current workflows.
#
# benchScale Deployment Script
#
# Deploys primals to a lab environment

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

LAB_NAME=""
MANIFEST=""
PRIMAL_BINS="../../../phase1bins"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"

usage() {
    cat << EOF
Usage: $0 --lab <lab-name> --manifest <manifest.yaml>

Deploy primals to a benchScale lab environment.

Required Arguments:
    --lab <name>            Lab name to deploy to
    --manifest <file>       BiomeOS manifest file to deploy

Optional Arguments:
    --primal-bins <dir>     Directory containing primal binaries (default: $PRIMAL_BINS)
    --help                  Show this help message

Examples:
    $0 --lab test-lab-01 --manifest ../templates/multi-tower-federation.biome.yaml
    $0 --lab lan-test --manifest ../templates/simple-lan.biome.yaml

EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab) LAB_NAME="$2"; shift 2 ;;
        --manifest) MANIFEST="$2"; shift 2 ;;
        --primal-bins) PRIMAL_BINS="$2"; shift 2 ;;
        --help) usage ;;
        *) echo -e "${RED}Error: Unknown option $1${NC}"; usage ;;
    esac
done

if [ -z "$LAB_NAME" ] || [ -z "$MANIFEST" ]; then
    echo -e "${RED}Error: --lab and --manifest are required${NC}"
    usage
fi

log() { echo -e "${GREEN}[benchScale]${NC} $1"; }
log_info() { echo -e "${BLUE}[benchScale]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[benchScale]${NC} $1"; }
log_error() { echo -e "${RED}[benchScale]${NC} $1"; }

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " benchScale Deployment"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Lab:      $LAB_NAME"
log_info "Manifest: $MANIFEST"
echo ""

# Check if lab exists
if [ ! -d "$STATE_DIR/$LAB_NAME" ]; then
    log_error "Lab not found: $LAB_NAME"
    log_info "Create it first with: ./create-lab.sh"
    exit 1
fi

# Get lab info
TOPOLOGY=$(grep "^topology:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')
HYPERVISOR=$(grep "^hypervisor:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')

log "Lab topology: $TOPOLOGY"
log "Hypervisor: $HYPERVISOR"
echo ""

# Deploy primals based on topology
log "Deploying primals..."

case $TOPOLOGY in
    simple-lan)
        log "Deploying to simple-lan topology..."
        
        # Node 1: Songbird + BearDog
        log "Deploying to node-1 (Songbird + BearDog)..."
        lxc file push "$PRIMAL_BINS/songbird-bin" "$LAB_NAME-node-1/root/songbird" || log_warn "Could not copy songbird binary"
        lxc file push "$PRIMAL_BINS/beardog-bin" "$LAB_NAME-node-1/root/beardog" || log_warn "Could not copy beardog binary"
        
        # Node 2: ToadStool + NestGate
        log "Deploying to node-2 (ToadStool + NestGate)..."
        lxc file push "$PRIMAL_BINS/toadstool-bin" "$LAB_NAME-node-2/root/toadstool" || log_warn "Could not copy toadstool binary"
        lxc file push "$PRIMAL_BINS/nestgate-bin" "$LAB_NAME-node-2/root/nestgate" || log_warn "Could not copy nestgate binary"
        
        log "✓ Binaries deployed"
        ;;
    
    p2p-3-tower)
        log "Deploying to 3-tower P2P topology..."
        
        # SF Tower: Songbird + BearDog
        log "Deploying to sf-tower..."
        lxc file push "$PRIMAL_BINS/songbird-bin" "$LAB_NAME-sf-tower/root/songbird" || log_warn "Could not copy songbird binary"
        lxc file push "$PRIMAL_BINS/beardog-bin" "$LAB_NAME-sf-tower/root/beardog" || log_warn "Could not copy beardog binary"
        
        # NY Tower: Songbird + ToadStool
        log "Deploying to ny-tower..."
        lxc file push "$PRIMAL_BINS/songbird-bin" "$LAB_NAME-ny-tower/root/songbird" || log_warn "Could not copy songbird binary"
        lxc file push "$PRIMAL_BINS/toadstool-bin" "$LAB_NAME-ny-tower/root/toadstool" || log_warn "Could not copy toadstool binary"
        
        # London Tower: Songbird + NestGate
        log "Deploying to london-tower..."
        lxc file push "$PRIMAL_BINS/songbird-bin" "$LAB_NAME-london-tower/root/songbird" || log_warn "Could not copy songbird binary"
        lxc file push "$PRIMAL_BINS/nestgate-bin" "$LAB_NAME-london-tower/root/nestgate" || log_warn "Could not copy nestgate binary"
        
        log "✓ Binaries deployed"
        ;;
    
    nat-traversal)
        log "Deploying to NAT traversal topology..."
        
        lxc file push "$PRIMAL_BINS/songbird-bin" "$LAB_NAME-relay-node/root/songbird" || log_warn "Could not copy songbird binary"
        lxc file push "$PRIMAL_BINS/beardog-bin" "$LAB_NAME-client-1/root/beardog" || log_warn "Could not copy beardog binary"
        lxc file push "$PRIMAL_BINS/toadstool-bin" "$LAB_NAME-client-2/root/toadstool" || log_warn "Could not copy toadstool binary"
        lxc file push "$PRIMAL_BINS/nestgate-bin" "$LAB_NAME-client-3/root/nestgate" || log_warn "Could not copy nestgate binary"
        
        log "✓ Binaries deployed"
        ;;
    
    *)
        log_warn "Unknown topology: $TOPOLOGY"
        log_warn "Manual deployment required"
        ;;
esac

# Update lab state
sed -i 's/^status:.*/status: deployed/' "$STATE_DIR/$LAB_NAME/info.yaml"
echo "deployed: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$STATE_DIR/$LAB_NAME/info.yaml"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "✅ Deployment complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Next steps:"
echo "  1. Start primals:  ./start-primals.sh --lab $LAB_NAME"
echo "  2. Run tests:      ./run-tests.sh --lab $LAB_NAME --test <test-name>"
echo "  3. Monitor:        ./monitor-lab.sh --lab $LAB_NAME"
echo ""

