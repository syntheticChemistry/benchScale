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
HYPERVISOR="docker"  # docker, lxd, qemu
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

# ── YAML parsing helpers ─────────────────────────────────────────────────────
# Minimal YAML parsing without yq dependency — extracts node names, images,
# env vars, and network conditions from benchScale topology YAML.

get_topology_network_name() {
    grep -E '^\s+name:' "$TOPOLOGY_FILE" | head -1 | sed 's/.*name:\s*//' | tr -d '"' | tr -d "'"
}

get_topology_subnet() {
    grep -E '^\s+subnet:' "$TOPOLOGY_FILE" | head -1 | sed 's/.*subnet:\s*//' | tr -d '"' | tr -d "'"
}

get_node_names() {
    grep -E '^\s+-\s+name:' "$TOPOLOGY_FILE" | sed 's/.*name:\s*//' | tr -d '"' | tr -d "'"
}

get_node_image() {
    local node="$1"
    awk -v node="$node" '
        /^\s+-?\s*name:/ { current = $NF; gsub(/["'"'"']/, "", current) }
        current == node && /^\s+image:/ { gsub(/["'"'"']/, "", $2); print $2 }
    ' "$TOPOLOGY_FILE"
}

get_node_env_block() {
    local node="$1"
    awk -v node="$node" '
        /^\s+-?\s*name:/ { current = $NF; gsub(/["'"'"']/, "", current); in_env=0 }
        current == node && /^\s+env:/ { in_env=1; next }
        current == node && in_env && /^\s+[A-Z_]+:/ {
            key = $1; gsub(/:$/, "", key)
            val = $2; for(i=3;i<=NF;i++) val = val " " $i
            gsub(/["'"'"']/, "", val)
            printf "%s=%s\n", key, val
        }
        current == node && in_env && /^\s+[a-z]/ && !/^\s+[A-Z_]+:/ { in_env=0 }
    ' "$TOPOLOGY_FILE"
}

get_node_network_conditions() {
    local node="$1"
    awk -v node="$node" '
        /^\s+-?\s*name:/ { current = $NF; gsub(/["'"'"']/, "", current); in_nc=0 }
        current == node && /network_conditions:/ { in_nc=1; next }
        current == node && in_nc && /latency_ms:/ { gsub(/["'"'"']/, "", $2); print "latency_ms=" $2 }
        current == node && in_nc && /packet_loss_percent:/ { gsub(/["'"'"']/, "", $2); print "loss=" $2 }
        current == node && in_nc && /bandwidth_kbps:/ { gsub(/["'"'"']/, "", $2); print "bw=" $2 }
        current == node && in_nc && /jitter_ms:/ { gsub(/["'"'"']/, "", $2); print "jitter=" $2 }
        current == node && in_nc && /^\s+[a-z]/ && !/^\s+(latency|packet|bandwidth|jitter|preset)/ { in_nc=0 }
    ' "$TOPOLOGY_FILE"
}

log "Parsing topology: $TOPOLOGY_FILE"

# ── Generic topology-driven lab creation ─────────────────────────────────────

NETWORK_NAME="${LAB_NAME}-net"
NODE_NAMES=()
NODE_IDS=()

while IFS= read -r node; do
    [ -z "$node" ] && continue
    NODE_NAMES+=("$node")
done < <(get_node_names)

NODE_COUNT=${#NODE_NAMES[@]}
if [ "$NODE_COUNT" -eq 0 ]; then
    log_error "No nodes found in topology: $TOPOLOGY_FILE"
    exit 1
fi

log "Found $NODE_COUNT nodes in topology"

case $HYPERVISOR in
    docker)
        log "Creating Docker network: $NETWORK_NAME"
        docker network create "$NETWORK_NAME" --driver bridge 2>/dev/null || \
            log_warn "Network $NETWORK_NAME may already exist"

        for node in "${NODE_NAMES[@]}"; do
            local_image="$(get_node_image "$node")"
            local_image="${local_image:-ubuntu}"

            case "$local_image" in
                ubuntu) docker_image="ubuntu:24.04" ;;
                alpine) docker_image="alpine:latest" ;;
                *)      docker_image="$local_image" ;;
            esac

            container_name="${LAB_NAME}-${node}"
            log "Creating container: $container_name (image: $docker_image)"

            env_args=()
            while IFS='=' read -r key val; do
                [ -z "$key" ] && continue
                env_args+=("-e" "${key}=${val}")
            done < <(get_node_env_block "$node")

            docker run -d \
                --name "$container_name" \
                --network "$NETWORK_NAME" \
                --hostname "$node" \
                --cap-add=NET_ADMIN \
                "${env_args[@]}" \
                "$docker_image" \
                sleep infinity 2>/dev/null && \
                log "  + $container_name" || \
                log_warn "  ! $container_name (may already exist)"

            NODE_IDS+=("$container_name")

            # Apply network conditions via tc if specified
            latency="" ; loss="" ; bw="" ; jitter=""
            while IFS='=' read -r nc_key nc_val; do
                case "$nc_key" in
                    latency_ms) latency="$nc_val" ;;
                    loss)       loss="$nc_val" ;;
                    bw)         bw="$nc_val" ;;
                    jitter)     jitter="$nc_val" ;;
                esac
            done < <(get_node_network_conditions "$node")

            if [ -n "$latency" ] && [ "$latency" != "0" ]; then
                tc_cmd="tc qdisc add dev eth0 root netem delay ${latency}ms"
                [ -n "$jitter" ] && [ "$jitter" != "0" ] && tc_cmd="$tc_cmd ${jitter}ms"
                [ -n "$loss" ] && [ "$loss" != "0" ] && [ "$loss" != "0.0" ] && tc_cmd="$tc_cmd loss ${loss}%"
                docker exec "$container_name" sh -c "$tc_cmd" 2>/dev/null && \
                    log "  tc: ${latency}ms latency, ${loss:-0}% loss" || \
                    log_warn "  tc: could not apply (may need iproute2 in image)"
            fi
        done

        log "Waiting for containers to stabilize..."
        sleep 2
        ;;

    lxd)
        for node in "${NODE_NAMES[@]}"; do
            container_name="${LAB_NAME}-${node}"
            log "Creating LXD container: $container_name"
            lxc launch ubuntu:24.04 "$container_name" || true
            NODE_IDS+=("$container_name")
        done

        sleep 5

        for node in "${NODE_NAMES[@]}"; do
            container_name="${LAB_NAME}-${node}"
            local latency=""
            while IFS='=' read -r nc_key nc_val; do
                case "$nc_key" in
                    latency_ms) latency="$nc_val" ;;
                esac
            done < <(get_node_network_conditions "$node")

            if [ -n "$latency" ] && [ "$latency" != "0" ]; then
                lxc exec "$container_name" -- tc qdisc add dev eth0 root netem delay "${latency}ms" 2>/dev/null || true
            fi
        done
        ;;

    qemu)
        log_warn "QEMU/KVM lab creation requires the benchscale Rust CLI"
        log_warn "Use: benchscale create $LAB_NAME $TOPOLOGY_FILE"
        log_warn "Or use --hypervisor docker for container-based labs"
        ;;
esac

# Save lab state
log "Saving lab state..."
cat > "$STATE_DIR/$LAB_NAME/info.yaml" <<EOF
name: $LAB_NAME
topology: $TOPOLOGY
hypervisor: $HYPERVISOR
network: $NETWORK_NAME
node_count: $NODE_COUNT
created: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
status: created
EOF

# Write node list for destroy-lab.sh
printf '%s\n' "${NODE_IDS[@]}" > "$STATE_DIR/$LAB_NAME/nodes.txt"

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
echo "  1. Deploy primals: ./deploy-ecoprimals.sh --lab $LAB_NAME"
echo "  2. Run validation: ../../springs/primalSpring/scripts/validate_local_lab.sh --topology $TOPOLOGY"
echo "  3. Tear down:      ./destroy-lab.sh --lab $LAB_NAME"
echo ""
log_info "Lab state saved to: $STATE_DIR/$LAB_NAME/"
echo ""

