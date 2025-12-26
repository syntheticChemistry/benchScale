#!/usr/bin/env bash
# benchScale Lab Creation Script
#
# Creates a lab environment from a topology manifest
# Supports: LXC/LXD, Docker, QEMU/KVM

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# Default values
TOPOLOGY=""
LAB_NAME=""
HYPERVISOR="lxd"  # lxd, docker, qemu
VERBOSE=false

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
TOPOLOGIES_DIR="$BENCHSCALE_ROOT/topologies"
STATE_DIR="$BENCHSCALE_ROOT/.state"

# Usage
usage() {
    cat << EOF
Usage: $0 --topology <topology> --name <lab-name> [options]

Create a benchScale lab environment from a topology manifest.

Required Arguments:
    --topology <name>       Topology to use (e.g., p2p-3-tower, simple-lan)
    --name <lab-name>       Name for this lab instance

Optional Arguments:
    --hypervisor <type>     Hypervisor to use: lxd (default), docker, qemu
    --verbose               Enable verbose output
    --help                  Show this help message

Examples:
    # Create a 3-tower P2P lab
    $0 --topology p2p-3-tower --name test-lab-01

    # Create a simple LAN lab with Docker
    $0 --topology simple-lan --name lan-test --hypervisor docker

    # Create a NAT traversal lab
    $0 --topology nat-traversal --name nat-test

Available Topologies:
EOF
    
    if [ -d "$TOPOLOGIES_DIR" ]; then
        for topo in "$TOPOLOGIES_DIR"/*.yaml; do
            [ -f "$topo" ] && echo "    - $(basename "$topo" .yaml)"
        done
    fi
    
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --topology)
            TOPOLOGY="$2"
            shift 2
            ;;
        --name)
            LAB_NAME="$2"
            shift 2
            ;;
        --hypervisor)
            HYPERVISOR="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            usage
            ;;
        *)
            echo -e "${RED}Error: Unknown option $1${NC}"
            usage
            ;;
    esac
done

# Validate required arguments
if [ -z "$TOPOLOGY" ] || [ -z "$LAB_NAME" ]; then
    echo -e "${RED}Error: --topology and --name are required${NC}"
    usage
fi

# Check if topology file exists
TOPOLOGY_FILE="$TOPOLOGIES_DIR/${TOPOLOGY}.yaml"
if [ ! -f "$TOPOLOGY_FILE" ]; then
    echo -e "${RED}Error: Topology file not found: $TOPOLOGY_FILE${NC}"
    exit 1
fi

# Create state directory
mkdir -p "$STATE_DIR/$LAB_NAME"

# Log function
log() {
    echo -e "${GREEN}[benchScale]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[benchScale]${NC} $1"
}

log_error() {
    echo -e "${RED}[benchScale]${NC} $1"
}

log_info() {
    echo -e "${BLUE}[benchScale]${NC} $1"
}

# Banner
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " benchScale Lab Creation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Lab Name:    $LAB_NAME"
log_info "Topology:    $TOPOLOGY"
log_info "Hypervisor:  $HYPERVISOR"
echo ""

# Check dependencies
log "Checking dependencies..."

case $HYPERVISOR in
    lxd)
        if ! command -v lxc &> /dev/null; then
            log_error "LXD not found. Install with: sudo snap install lxd"
            exit 1
        fi
        log "✓ LXD found"
        ;;
    docker)
        if ! command -v docker &> /dev/null; then
            log_error "Docker not found. Install from: https://docs.docker.com/get-docker/"
            exit 1
        fi
        log "✓ Docker found"
        ;;
    qemu)
        if ! command -v qemu-system-x86_64 &> /dev/null; then
            log_error "QEMU not found. Install with: sudo apt install qemu-kvm"
            exit 1
        fi
        log "✓ QEMU found"
        ;;
    *)
        log_error "Unknown hypervisor: $HYPERVISOR"
        exit 1
        ;;
esac

# Parse topology (simplified for now - would use yq in production)
log "Parsing topology: $TOPOLOGY_FILE"

# For now, we'll create a simple implementation
# In production, this would parse YAML and create VMs dynamically

case $TOPOLOGY in
    simple-lan)
        log "Creating simple LAN topology (2 nodes)..."
        
        if [ "$HYPERVISOR" = "lxd" ]; then
            log "Creating node-1..."
            lxc launch ubuntu:22.04 "$LAB_NAME-node-1" || true
            
            log "Creating node-2..."
            lxc launch ubuntu:22.04 "$LAB_NAME-node-2" || true
            
            # Wait for containers to start
            sleep 5
            
            # Configure network (simplified)
            log "Configuring network..."
            lxc exec "$LAB_NAME-node-1" -- ip addr add 192.168.100.10/24 dev eth0 || true
            lxc exec "$LAB_NAME-node-2" -- ip addr add 192.168.100.20/24 dev eth0 || true
        fi
        ;;
    
    p2p-3-tower)
        log "Creating 3-tower P2P topology (3 nodes)..."
        
        if [ "$HYPERVISOR" = "lxd" ]; then
            log "Creating sf-tower..."
            lxc launch ubuntu:22.04 "$LAB_NAME-sf-tower" || true
            
            log "Creating ny-tower..."
            lxc launch ubuntu:22.04 "$LAB_NAME-ny-tower" || true
            
            log "Creating london-tower..."
            lxc launch ubuntu:22.04 "$LAB_NAME-london-tower" || true
            
            # Wait for containers to start
            sleep 5
            
            # Configure network
            log "Configuring network..."
            lxc exec "$LAB_NAME-sf-tower" -- ip addr add 192.168.100.10/24 dev eth0 || true
            lxc exec "$LAB_NAME-ny-tower" -- ip addr add 192.168.100.20/24 dev eth0 || true
            lxc exec "$LAB_NAME-london-tower" -- ip addr add 192.168.100.30/24 dev eth0 || true
            
            # Add latency simulation using tc (traffic control)
            log "Configuring network latency..."
            lxc exec "$LAB_NAME-sf-tower" -- tc qdisc add dev eth0 root netem delay 20ms || true
            lxc exec "$LAB_NAME-ny-tower" -- tc qdisc add dev eth0 root netem delay 20ms || true
            lxc exec "$LAB_NAME-london-tower" -- tc qdisc add dev eth0 root netem delay 70ms || true
        fi
        ;;
    
    nat-traversal)
        log "Creating NAT traversal topology (4 nodes)..."
        
        if [ "$HYPERVISOR" = "lxd" ]; then
            log "Creating relay-node..."
            lxc launch ubuntu:22.04 "$LAB_NAME-relay-node" || true
            
            log "Creating client-1..."
            lxc launch ubuntu:22.04 "$LAB_NAME-client-1" || true
            
            log "Creating client-2..."
            lxc launch ubuntu:22.04 "$LAB_NAME-client-2" || true
            
            log "Creating client-3..."
            lxc launch ubuntu:22.04 "$LAB_NAME-client-3" || true
            
            sleep 5
        fi
        ;;
    
    *)
        log_warn "Custom topology: $TOPOLOGY"
        log_warn "Generic lab creation not yet implemented"
        log_warn "Please create VMs manually or use a predefined topology"
        ;;
esac

# Save lab state
log "Saving lab state..."
cat > "$STATE_DIR/$LAB_NAME/info.yaml" <<EOF
name: $LAB_NAME
topology: $TOPOLOGY
hypervisor: $HYPERVISOR
created: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
status: created
EOF

# Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "✅ Lab created successfully!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Lab Name:  $LAB_NAME"
log_info "Topology:  $TOPOLOGY"
log_info "Hypervisor: $HYPERVISOR"
echo ""
log_info "Next steps:"
echo "  1. Deploy primals: ./deploy-to-lab.sh --lab $LAB_NAME --manifest <manifest.yaml>"
echo "  2. Run tests:      ./run-tests.sh --lab $LAB_NAME --test <test-name>"
echo "  3. Monitor:        ./monitor-lab.sh --lab $LAB_NAME"
echo "  4. Tear down:      ./destroy-lab.sh --lab $LAB_NAME"
echo ""
log_info "Lab state saved to: $STATE_DIR/$LAB_NAME/"
echo ""

