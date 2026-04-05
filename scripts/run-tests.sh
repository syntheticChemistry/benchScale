#!/usr/bin/env bash
# Current: integration tests against a live lab via primalSpring (see scripts/README.md).
#
# benchScale Test Runner — delegates to primalSpring validation pipeline
#
# This script wraps primalSpring's validate_local_lab.sh to run ecoPrimals
# experiments against a running benchScale lab. The actual test surface is
# the primalSpring experiment suite (exp073, exp074, etc.).
#
# Usage:
#   ./run-tests.sh --lab <lab-name>
#   ./run-tests.sh --topology <topology-name>
#
# See also: scripts/archive/run-tests-stub.sh (original placeholder)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHSCALE_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="$BENCHSCALE_ROOT/.state"
PRIMALSPRING_ROOT="$BENCHSCALE_ROOT/../../springs/primalSpring"

GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

log()      { echo -e "${GREEN}[benchScale]${NC} $1"; }
log_info() { echo -e "${BLUE}[benchScale]${NC} $1"; }
log_err()  { echo -e "${RED}[benchScale]${NC} $1"; }

LAB_NAME=""
TOPOLOGY=""
TIMEOUT=60
EXTRA_ARGS=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --lab)       LAB_NAME="$2"; shift 2 ;;
        --topology)  TOPOLOGY="$2"; shift 2 ;;
        --timeout)   TIMEOUT="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: $0 --lab <lab-name> | --topology <topology-name>"
            echo ""
            echo "  --lab <name>       Use topology from an existing lab's state"
            echo "  --topology <name>  Specify topology directly"
            echo "  --timeout <secs>   Per-experiment timeout (default: 60)"
            exit 0
            ;;
        *)
            EXTRA_ARGS="$EXTRA_ARGS $1"
            shift
            ;;
    esac
done

if [ -n "$LAB_NAME" ] && [ -z "$TOPOLOGY" ]; then
    if [ -f "$STATE_DIR/$LAB_NAME/info.yaml" ]; then
        TOPOLOGY=$(grep "^topology:" "$STATE_DIR/$LAB_NAME/info.yaml" | awk '{print $2}')
        log "Resolved topology from lab state: $TOPOLOGY"
    else
        log_err "Lab not found: $LAB_NAME (no state at $STATE_DIR/$LAB_NAME/)"
        exit 1
    fi
fi

if [ -z "$TOPOLOGY" ]; then
    log_err "Either --lab or --topology is required"
    exit 1
fi

VALIDATE_SCRIPT="$PRIMALSPRING_ROOT/scripts/validate_local_lab.sh"
if [ ! -f "$VALIDATE_SCRIPT" ]; then
    log_err "validate_local_lab.sh not found at: $VALIDATE_SCRIPT"
    log_info "Ensure primalSpring is checked out at: $PRIMALSPRING_ROOT"
    exit 1
fi

log "Running primalSpring validation for topology: $TOPOLOGY"
log_info "Timeout: ${TIMEOUT}s per experiment"
echo ""

exec "$VALIDATE_SCRIPT" --topology "$TOPOLOGY" --timeout "$TIMEOUT" $EXTRA_ARGS
