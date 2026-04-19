#!/usr/bin/env bash
# Current: primary script for deploying ecoPrimals artifacts into a lab (see scripts/README.md).
#
# deploy-ecoprimals.sh — Deploy ecoPrimals NUCLEUS primals into a benchScale lab
#
# Copies musl-static primal binaries from plasmidBin, deploy graphs from
# primalSpring, and launch profiles into each lab node. Then starts primals
# on their well-known TCP ports per the topology YAML's PRIMALS env metadata.
#
# All 13 NUCLEUS primals are supported:
#   Tower:      beardog, songbird
#   Node:       toadstool, barracuda, coralreef
#   Nest:       nestgate, rhizocrypt, loamspine, sweetgrass
#   Meta-tier:  biomeos, squirrel, petaltongue
#   Defense:    skunkbat
#
# Usage:
#   ./deploy-ecoprimals.sh --lab <lab-name> --plasmidbin <path> [--graphs <path>] [--seed <family-seed>]
#
# Requires: docker (or lxc) depending on lab hypervisor

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"

LAB_NAME=""
PLASMIDBIN_DIR=""
GRAPHS_DIR=""
FAMILY_SEED=""
DEPLOY_DIR="/opt/ecoprimals"
DEPLOY_ARCH="x86_64"

usage() {
    cat << EOF
Usage: $0 --lab <lab-name> --plasmidbin <path> [options]

Deploy ecoPrimals NUCLEUS primal binaries into a benchScale lab.

Required Arguments:
    --lab <name>            Lab name (must exist via create-lab.sh)
    --plasmidbin <dir>      Path to plasmidBin/ root (contains ports.env, binaries)

Optional Arguments:
    --graphs <dir>          Path to primalSpring/graphs/ (default: auto-detect)
    --seed <string>         Family seed for primal identity (default: lab name)
    --deploy-dir <path>     Remote install path (default: $DEPLOY_DIR)
    --arch <arch>           Target architecture: x86_64 or aarch64 (default: x86_64)
    --help                  Show this help message

Examples:
    $0 --lab tower-test --plasmidbin ../../plasmidBin
    $0 --lab nucleus-3 --plasmidbin /home/user/ecoPrimals/infra/plasmidBin --seed my-family

EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab) LAB_NAME="$2"; shift 2 ;;
        --plasmidbin) PLASMIDBIN_DIR="$2"; shift 2 ;;
        --graphs) GRAPHS_DIR="$2"; shift 2 ;;
        --seed) FAMILY_SEED="$2"; shift 2 ;;
        --deploy-dir) DEPLOY_DIR="$2"; shift 2 ;;
        --arch) DEPLOY_ARCH="$2"; shift 2 ;;
        --help) usage ;;
        *) echo -e "${RED}Error: Unknown option $1${NC}"; usage ;;
    esac
done

if [ -z "$LAB_NAME" ] || [ -z "$PLASMIDBIN_DIR" ]; then
    echo -e "${RED}Error: --lab and --plasmidbin are required${NC}"
    usage
fi

if [ ! -d "$STATE_DIR/$LAB_NAME" ]; then
    echo -e "${RED}Error: Lab not found: $LAB_NAME${NC}"
    echo "Create it first with: ./create-lab.sh --topology <topo> --name $LAB_NAME --hypervisor docker"
    exit 1
fi

FAMILY_SEED="${FAMILY_SEED:-$LAB_NAME}"

if [ -z "$GRAPHS_DIR" ]; then
    for candidate in \
        "$SCRIPT_DIR/../../../springs/primalSpring/graphs" \
        "$SCRIPT_DIR/../../../../springs/primalSpring/graphs"; do
        if [ -d "$candidate" ]; then
            GRAPHS_DIR="$(cd "$candidate" && pwd)"
            break
        fi
    done
fi

PLASMIDBIN_DIR="$(cd "$PLASMIDBIN_DIR" && pwd)"

# Source port defaults
if [ -f "$PLASMIDBIN_DIR/ports.env" ]; then
    # shellcheck source=../../plasmidBin/ports.env
    source "$PLASMIDBIN_DIR/ports.env"
fi

log()      { echo -e "${GREEN}[deploy]${NC} $1"; }
log_info() { echo -e "${BLUE}[deploy]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[deploy]${NC} $1"; }
log_err()  { echo -e "${RED}[deploy]${NC} $1"; }

HYPERVISOR=$(grep "^hypervisor:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')
TOPOLOGY=$(grep "^topology:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')
TOPOLOGY_FILE="$BENCHSCALE_ROOT/topologies/${TOPOLOGY}.yaml"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " ecoPrimals Lab Deployment"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Lab:         $LAB_NAME"
log_info "Topology:    $TOPOLOGY"
log_info "Hypervisor:  $HYPERVISOR"
log_info "plasmidBin:  $PLASMIDBIN_DIR"
log_info "Graphs:      ${GRAPHS_DIR:-none}"
log_info "Family seed: $FAMILY_SEED"
log_info "Deploy dir:  $DEPLOY_DIR"
log_info "Target arch: $DEPLOY_ARCH"
echo ""

# ── Container helpers ────────────────────────────────────────────────────────

container_name() {
    echo "${LAB_NAME}-${1}"
}

exec_in_node() {
    local node="$1"; shift
    local cname
    cname="$(container_name "$node")"
    case "$HYPERVISOR" in
        docker) docker exec "$cname" "$@" ;;
        lxd)    lxc exec "$cname" -- "$@" ;;
        *)      log_err "Unsupported hypervisor: $HYPERVISOR"; return 1 ;;
    esac
}

copy_to_node() {
    local node="$1" src="$2" dst="$3"
    local cname
    cname="$(container_name "$node")"
    case "$HYPERVISOR" in
        docker) docker cp "$src" "${cname}:${dst}" ;;
        lxd)    lxc file push "$src" "${cname}${dst}" ;;
        *)      log_err "Unsupported hypervisor: $HYPERVISOR"; return 1 ;;
    esac
}

# ── Parse node names from topology YAML ──────────────────────────────────────
# Minimal YAML parsing — extracts node names from "- name: <value>" lines

get_node_names() {
    grep -E '^\s+-\s+name:' "$TOPOLOGY_FILE" | sed 's/.*name:\s*//' | tr -d '"' | tr -d "'"
}

get_node_env() {
    local node="$1" key="$2"
    awk -v node="$node" -v key="$key" '
        /^\s+-\s+name:/ { current = $NF; gsub(/["'"'"']/, "", current) }
        current == node && $1 == key":" {
            val = substr($0, index($0, ":") + 1)
            gsub(/^[ \t]+/, "", val)
            gsub(/["'"'"']/, "", val)
            print val
        }
    ' "$TOPOLOGY_FILE"
}

# ── Deploy to each node ─────────────────────────────────────────────────────

deployed=0
failed=0

deploy_node() {
    local node="$1"
    log "Deploying to $node..."

    # Create deploy directory
    exec_in_node "$node" mkdir -p "$DEPLOY_DIR/bin" "$DEPLOY_DIR/graphs" "$DEPLOY_DIR/config" 2>/dev/null || true

    local primals_env
    primals_env="$(get_node_env "$node" "PRIMALS")"
    if [ -z "$primals_env" ]; then
        log_warn "  No PRIMALS env for $node, skipping binary deploy"
        return
    fi

    # Determine target triple for binary resolution (genomeBin layout)
    local target_arch="${DEPLOY_ARCH:-x86_64}"
    local target_triple=""
    local arch_subdir=""
    case "$target_arch" in
        x86_64)  target_triple="x86_64-unknown-linux-musl" ;;
        aarch64) target_triple="aarch64-unknown-linux-musl"; arch_subdir="/aarch64" ;;
        armv7)   target_triple="armv7-unknown-linux-musleabihf" ;;
        *)       target_triple="${target_arch}-unknown-linux-musl" ;;
    esac

    # Copy primal binaries — try genomeBin target-triple layout first, fall back to legacy
    for primal in $primals_env; do
        local bin_path=""
        for candidate in \
            "$PLASMIDBIN_DIR/primals/${target_triple}/$primal" \
            "$PLASMIDBIN_DIR/primals${arch_subdir}/$primal" \
            "$PLASMIDBIN_DIR/primals/$primal" \
            "$PLASMIDBIN_DIR/$primal"; do
            # Resolve symlinks so docker cp gets a real file
            if [ -L "$candidate" ]; then
                candidate="$(readlink -f "$candidate")"
            fi
            if [ -f "$candidate" ] && [ -x "$candidate" ]; then
                bin_path="$candidate"
                break
            fi
        done

        if [ -n "$bin_path" ]; then
            copy_to_node "$node" "$bin_path" "$DEPLOY_DIR/bin/$primal" && \
                log "  + $primal" || \
                log_warn "  ! $primal (copy failed)"
        else
            log_warn "  - $primal (binary not found in plasmidBin)"
        fi
    done

    # Copy graphs if available
    if [ -n "$GRAPHS_DIR" ] && [ -d "$GRAPHS_DIR" ]; then
        for graph_file in "$GRAPHS_DIR"/**/*.toml "$GRAPHS_DIR"/*.toml; do
            [ -f "$graph_file" ] || continue
            local relpath="${graph_file#"$GRAPHS_DIR/"}"
            copy_to_node "$node" "$graph_file" "$DEPLOY_DIR/graphs/$relpath" 2>/dev/null || true
        done
        log "  + graphs copied"
    fi

    # Copy launch profiles
    local profiles_path
    for candidate in \
        "$SCRIPT_DIR/../../../springs/primalSpring/config/primal_launch_profiles.toml" \
        "$SCRIPT_DIR/../../../../springs/primalSpring/config/primal_launch_profiles.toml"; do
        if [ -f "$candidate" ]; then
            profiles_path="$(cd "$(dirname "$candidate")" && pwd)/$(basename "$candidate")"
            break
        fi
    done
    if [ -n "${profiles_path:-}" ]; then
        copy_to_node "$node" "$profiles_path" "$DEPLOY_DIR/config/primal_launch_profiles.toml" 2>/dev/null || true
    fi

    # Copy ports.env
    if [ -f "$PLASMIDBIN_DIR/ports.env" ]; then
        copy_to_node "$node" "$PLASMIDBIN_DIR/ports.env" "$DEPLOY_DIR/config/ports.env" 2>/dev/null || true
    fi

    # Write family seed — prefer per-node FAMILY_ID from topology, fallback to global seed
    local node_family_id
    node_family_id="$(get_node_env "$node" "FAMILY_ID")"
    node_family_id="${node_family_id:-$FAMILY_SEED}"
    exec_in_node "$node" sh -c "echo '$node_family_id' > $DEPLOY_DIR/.family.seed" 2>/dev/null || true

    # Make binaries executable
    exec_in_node "$node" chmod +x "$DEPLOY_DIR/bin/"* 2>/dev/null || true

    ((deployed++)) || true
}

# ── Start primals on each node ───────────────────────────────────────────────

resolve_tower_host() {
    local node="$1"
    local tower_host
    tower_host="$(get_node_env "$node" "TOWER_HOST")"
    if [ -n "$tower_host" ]; then
        echo "$tower_host"
    else
        echo "127.0.0.1"
    fi
}

build_primal_env() {
    local node="$1" primal="$2" family_id="$3"
    local tower_host
    tower_host="$(resolve_tower_host "$node")"

    local tower_beardog_port tower_songbird_port tower_biomeos_port
    tower_beardog_port="$(get_node_env "$node" "BEARDOG_PORT")"
    tower_songbird_port="$(get_node_env "$node" "SONGBIRD_PORT")"
    tower_biomeos_port="$(get_node_env "$node" "BIOMEOS_PORT")"
    tower_beardog_port="${tower_beardog_port:-9100}"
    tower_songbird_port="${tower_songbird_port:-9200}"
    tower_biomeos_port="${tower_biomeos_port:-9800}"

    local env_str="FAMILY_ID='$family_id' RUST_LOG=info"

    case "$primal" in
        beardog)
            env_str="$env_str NODE_ID=tower1"
            env_str="$env_str FAMILY_SEED='$family_id'"
            env_str="$env_str BEARDOG_FAMILY_SEED='$family_id'"
            ;;
        songbird)
            env_str="$env_str BEARDOG_MODE=direct"
            env_str="$env_str SONGBIRD_SECURITY_PROVIDER=beardog"
            env_str="$env_str SONGBIRD_DISCOVERY_MODE=disabled"
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            ;;
        nestgate)
            env_str="$env_str NESTGATE_FAMILY_ID='$family_id'"
            env_str="$env_str NESTGATE_JWT_SECRET='benchscale-dev-jwt-${family_id}'"
            ;;
        toadstool)
            env_str="$env_str TOADSTOOL_SECURITY_WARNING_ACKNOWLEDGED=1"
            env_str="$env_str TOADSTOOL_FAMILY_ID='$family_id'"
            env_str="$env_str NESTGATE_SOCKET=tcp://${tower_host}:${NESTGATE_PORT:-9300}"
            ;;
        barracuda)
            env_str="$env_str BARRACUDA_FAMILY_ID='$family_id'"
            env_str="$env_str TOADSTOOL_SOCKET=tcp://${tower_host}:${TOADSTOOL_PORT:-9400}"
            ;;
        coralreef)
            env_str="$env_str CORALREEF_FAMILY_ID='$family_id'"
            env_str="$env_str TOADSTOOL_SOCKET=tcp://${tower_host}:${TOADSTOOL_PORT:-9400}"
            env_str="$env_str BARRACUDA_SOCKET=tcp://${tower_host}:${BARRACUDA_PORT:-9740}"
            ;;
        rhizocrypt)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            ;;
        loamspine)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            env_str="$env_str RHIZOCRYPT_SOCKET=tcp://${tower_host}:${RHIZOCRYPT_PORT:-9700}"
            ;;
        sweetgrass)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            ;;
        squirrel)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            env_str="$env_str SONGBIRD_SOCKET=tcp://${tower_host}:${tower_songbird_port}"
            env_str="$env_str TOADSTOOL_SOCKET=tcp://${tower_host}:${TOADSTOOL_PORT:-9400}"
            env_str="$env_str NESTGATE_SOCKET=tcp://${tower_host}:${NESTGATE_PORT:-9300}"
            env_str="$env_str BIOMEOS_SOCKET_DIR=tcp://${tower_host}:${tower_biomeos_port}"
            ;;
        petaltongue)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            env_str="$env_str BIOMEOS_SOCKET_DIR=tcp://${tower_host}:${tower_biomeos_port}"
            ;;
        biomeos)
            env_str="$env_str BEARDOG_SOCKET=tcp://${tower_host}:${tower_beardog_port}"
            local bm_socket_dir
            bm_socket_dir="$(get_node_env "$node" "BIOMEOS_SOCKET_DIR")"
            [ -n "$bm_socket_dir" ] && env_str="$env_str BIOMEOS_SOCKET_DIR='$bm_socket_dir'"
            ;;
    esac

    echo "$env_str"
}

apply_launch_profile() {
    local primal="$1"
    local profiles_toml="$2"
    local extra_env=""

    if [ ! -f "$profiles_toml" ]; then
        echo ""
        return
    fi

    local in_section=false
    while IFS= read -r line; do
        if [[ "$line" =~ ^\[profiles\."$primal"\.extra_env\] ]]; then
            in_section=true
            continue
        fi
        if $in_section; then
            [[ "$line" =~ ^\[ ]] && break
            [[ -z "$line" || "$line" =~ ^# ]] && continue
            local key val
            key="$(echo "$line" | cut -d= -f1 | tr -d ' ')"
            val="$(echo "$line" | cut -d= -f2- | tr -d ' "'"'")"
            [ -n "$key" ] && [ -n "$val" ] && extra_env="$extra_env $key='$val'"
        fi
    done < "$profiles_toml"

    echo "$extra_env"
}

build_launch_cmd() {
    local primal="$1" port="$2" family_id="$3"
    case "$primal" in
        beardog)
            echo "$DEPLOY_DIR/bin/beardog server --port $port --family-id '$family_id'"
            ;;
        songbird)
            echo "SONGBIRD_PORT=$port $DEPLOY_DIR/bin/songbird server --port $port"
            ;;
        nestgate)
            echo "$DEPLOY_DIR/bin/nestgate server --port $port --dev --family-id '$family_id'"
            ;;
        toadstool)
            echo "$DEPLOY_DIR/bin/toadstool server --port $port"
            ;;
        barracuda)
            echo "$DEPLOY_DIR/bin/barracuda server --port $port"
            ;;
        coralreef)
            echo "$DEPLOY_DIR/bin/coralreef server --port $port"
            ;;
        rhizocrypt)
            echo "$DEPLOY_DIR/bin/rhizocrypt server --port $port --host 0.0.0.0"
            ;;
        loamspine)
            echo "$DEPLOY_DIR/bin/loamspine server --port $port"
            ;;
        sweetgrass)
            echo "$DEPLOY_DIR/bin/sweetgrass server --port $port"
            ;;
        squirrel)
            echo "$DEPLOY_DIR/bin/squirrel server --port $port"
            ;;
        petaltongue)
            echo "PETALTONGUE_MODE=server PETALTONGUE_HEADLESS=true $DEPLOY_DIR/bin/petaltongue server --port $port"
            ;;
        biomeos)
            echo "$DEPLOY_DIR/bin/biomeos neural-api --graphs-dir $DEPLOY_DIR/graphs --port $port --family-id '$family_id' --tcp-only"
            ;;
        *)
            echo "$DEPLOY_DIR/bin/$primal server --port $port --family-id '$family_id'"
            ;;
    esac
}

start_node_primals() {
    local node="$1"
    log "Starting primals on $node..."

    local primals_env
    primals_env="$(get_node_env "$node" "PRIMALS")"
    [ -z "$primals_env" ] && return

    local family_id
    family_id="$(get_node_env "$node" "FAMILY_ID")"
    family_id="${family_id:-$FAMILY_SEED}"

    for primal in $primals_env; do
        local port_var="${primal^^}_PORT"
        local port
        port="$(get_node_env "$node" "${port_var}")"
        [ -z "$port" ] && port="$(get_node_env "$node" "$(echo "$primal" | tr '[:lower:]' '[:upper:]')_PORT")"

        if [ -z "$port" ]; then
            log_warn "  No port for $primal on $node, skipping start"
            continue
        fi

        local primal_env
        primal_env="$(build_primal_env "$node" "$primal" "$family_id")"

        local profiles_toml="$DEPLOY_DIR/config/primal_launch_profiles.toml"
        local profile_env
        profile_env="$(apply_launch_profile "$primal" "$SCRIPT_DIR/../../../springs/primalSpring/config/primal_launch_profiles.toml")"
        primal_env="$primal_env $profile_env"

        local launch_cmd
        launch_cmd="$(build_launch_cmd "$primal" "$port" "$family_id")"

        exec_in_node "$node" sh -c \
            "export $primal_env; \
             nohup sh -c '$launch_cmd' \
             > /var/log/${primal}.log 2>&1 &" 2>/dev/null && \
            log "  + $primal listening on :$port (env wired, profile merged)" || \
            log_warn "  ! $primal failed to start (binary may be missing)"
    done
}

# ── Health check ─────────────────────────────────────────────────────────────

get_container_ip() {
    local node="$1"
    local cname
    cname="$(container_name "$node")"
    case "$HYPERVISOR" in
        docker) docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$cname" 2>/dev/null ;;
        lxd)    lxc list "$cname" --format csv -c 4 2>/dev/null | cut -d' ' -f1 ;;
        *)      echo "127.0.0.1" ;;
    esac
}

probe_tcp_jsonrpc() {
    local ip="$1" port="$2"
    echo '{"jsonrpc":"2.0","method":"health.liveness","params":{},"id":1}' | \
        timeout 3 nc -w 2 "$ip" "$port" 2>/dev/null | grep -q '"result"'
}

probe_http_health() {
    local ip="$1" port="$2"
    printf "GET /health HTTP/1.1\r\nHost: %s:%s\r\nConnection: close\r\n\r\n" "$ip" "$port" | \
        timeout 3 nc -w 2 "$ip" "$port" 2>/dev/null | grep -q "200"
}

health_check_node() {
    local node="$1"
    local primals_env
    primals_env="$(get_node_env "$node" "PRIMALS")"
    [ -z "$primals_env" ] && return

    local node_ip
    node_ip="$(get_container_ip "$node")"

    for primal in $primals_env; do
        local port_var="${primal^^}_PORT"
        local port
        port="$(get_node_env "$node" "${port_var}")"
        [ -z "$port" ] && continue

        local live=false
        case "$primal" in
            songbird)
                probe_http_health "$node_ip" "$port" && live=true ;;
            beardog|sweetgrass)
                # BTSP-secured: TCP probes are blocked by design.
                # Check process is running instead of TCP health.
                local cname
                cname="$(container_name "$node")"
                if docker exec "$cname" sh -c "ls /proc/*/exe 2>/dev/null | xargs readlink 2>/dev/null | grep -q $primal" 2>/dev/null; then
                    log "  ✓ $primal :$port  RUNNING (BTSP secured)"
                    continue
                fi
                ;;
            barracuda)
                # barraCuda requires GPU hardware — may crash in Docker.
                probe_tcp_jsonrpc "$node_ip" "$port" && live=true
                if [ "$live" != true ]; then
                    log_warn "  ⚡ $primal :$port  NO_GPU (expected in Docker)"
                    continue
                fi
                ;;
            *)
                probe_tcp_jsonrpc "$node_ip" "$port" && live=true ;;
        esac

        if [ "$live" = true ]; then
            log "  ✓ $primal :$port  LIVE"
        else
            log_warn "  ✗ $primal :$port  DOWN (may need more startup time)"
        fi
    done
}

# ── Main ─────────────────────────────────────────────────────────────────────

log "Phase 1: Deploying binaries + config..."
echo ""

while IFS= read -r node; do
    deploy_node "$node"
done < <(get_node_names)

echo ""
log "Phase 2: Starting primals..."
echo ""

while IFS= read -r node; do
    start_node_primals "$node"
done < <(get_node_names)

# ── Phase 2.5: Align biomeOS socket discovery with primal sockets ─────────

align_biomeos_sockets() {
    local node="$1"
    local primals_env
    primals_env="$(get_node_env "$node" "PRIMALS")"
    echo "$primals_env" | grep -qw biomeos || return 0

    local family_id
    family_id="$(get_node_env "$node" "FAMILY_ID")"
    family_id="${family_id:-$FAMILY_SEED}"

    exec_in_node "$node" sh -c "
        mkdir -p /tmp/biomeos /tmp/biomeos-default
        # Symlink all family-scoped primal sockets from /tmp/ into /tmp/biomeos/
        # so biomeOS can find them when it looks in its expected directories.
        for src in /tmp/*-${family_id}.sock /tmp/*-${family_id}-*.sock; do
            [ -S \"\$src\" ] || continue
            base=\$(basename \"\$src\")
            ln -sf \"\$src\" /tmp/biomeos/\$base 2>/dev/null
            ln -sf \"\$src\" /tmp/biomeos-default/\$base 2>/dev/null
        done
        # Plain sockets (songbird.sock etc.) also get aliased
        for src in /tmp/songbird.sock /tmp/beardog.sock; do
            [ -S \"\$src\" ] || continue
            base=\$(basename \"\$src\")
            ln -sf \"\$src\" /tmp/biomeos/\$base 2>/dev/null
            ln -sf \"\$src\" /tmp/biomeos-default/\$base 2>/dev/null
        done
    " 2>/dev/null && log "  + $node: biomeOS socket symlinks aligned" || true
}

echo ""
log "Phase 2.5: Aligning biomeOS socket discovery..."
while IFS= read -r node; do
    align_biomeos_sockets "$node"
done < <(get_node_names)

echo ""
log "Phase 3: Health check (15s grace, 3 retries)..."
sleep 15

HEALTH_RETRIES=3
HEALTH_RETRY_DELAY=10
for attempt in $(seq 1 $HEALTH_RETRIES); do
    all_live=true
    while IFS= read -r node; do
        health_check_node "$node" || all_live=false
    done < <(get_node_names)
    if [ "$all_live" = true ] || [ "$attempt" -eq "$HEALTH_RETRIES" ]; then
        break
    fi
    log_info "Retry $attempt/$HEALTH_RETRIES in ${HEALTH_RETRY_DELAY}s (biomeOS may still be bootstrapping)..."
    sleep "$HEALTH_RETRY_DELAY"
done

# ── Phase 4: Load graphs into biomeOS ─────────────────────────────────────────

load_graphs_on_node() {
    local node="$1"
    local primals_env
    primals_env="$(get_node_env "$node" "PRIMALS")"
    echo "$primals_env" | grep -qw biomeos || return 0

    local node_ip bm_port
    node_ip="$(get_container_ip "$node")"
    bm_port="$(get_node_env "$node" "BIOMEOS_PORT")"
    bm_port="${bm_port:-9800}"

    local graph_dir="$DEPLOY_DIR/graphs"
    local loaded=0

    local graph_files
    graph_files="$(exec_in_node "$node" find "$graph_dir" -name '*.toml' -type f 2>/dev/null)" || return 0

    for graph_path in $graph_files; do
        local graph_id
        graph_id="$(basename "$graph_path" .toml)"
        local req="{\"jsonrpc\":\"2.0\",\"method\":\"graph.load\",\"params\":{\"path\":\"$graph_path\",\"id\":\"$graph_id\"},\"id\":1}"
        local resp
        resp="$(echo "$req" | timeout 5 nc -w 3 "$node_ip" "$bm_port" 2>/dev/null)" || continue
        if echo "$resp" | grep -q '"result"'; then
            ((loaded++)) || true
        fi
    done

    if [ "$loaded" -gt 0 ]; then
        log "  + $node: $loaded graph(s) loaded into biomeOS"
    else
        log_info "  $node: biomeOS graph.load returned no results (graphs may auto-load from --graphs-dir)"
    fi
}

echo ""
log "Phase 4: Loading graphs into biomeOS..."
while IFS= read -r node; do
    load_graphs_on_node "$node"
done < <(get_node_names)

# Update lab state
sed -i 's/^status:.*/status: deployed/' "$STATE_DIR/$LAB_NAME/info.yaml" 2>/dev/null || true
echo "deployed: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$STATE_DIR/$LAB_NAME/info.yaml"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
log "Deployment complete: $deployed nodes provisioned"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
log_info "Next steps:"
echo "  1. Validate:  ../../../springs/primalSpring/scripts/validate_local_lab.sh --lab $LAB_NAME"
echo "  2. Run exp:   REMOTE_GATE_HOST=<node-ip> cargo run --bin exp074_cross_gate_health"
echo "  3. Tear down: ./destroy-lab.sh --lab $LAB_NAME --force"
echo ""
