#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# test-waterfall-local.sh — Local (no Docker) validation of WaterFall gate-sync
#
# Tests cascade-pull.sh gate identity, profile filtering, source routing,
# manifest consistency, parity check, and .gate file using --dry-run.
# Runs entirely on the local workspace — no containers, no network, no
# Forgejo required.
#
# Usage:
#   ./test-waterfall-local.sh              # Run all tests
#   ./test-waterfall-local.sh --verbose    # Show full cascade-pull output

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ECO_ROOT="${ECOPRIMALS_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
WH_DIR="$ECO_ROOT/infra/wateringHole"
CASCADE="$WH_DIR/cascade-pull.sh"
MANIFEST="$WH_DIR/ecosystem_manifest.toml"

GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

VERBOSE=false
[[ "${1:-}" == "--verbose" ]] && VERBOSE=true

PASS=0
FAIL=0
TESTS=()

pass() { PASS=$((PASS + 1)); TESTS+=("PASS: $1"); echo -e "  ${GREEN}PASS${NC} $1"; }
fail() { FAIL=$((FAIL + 1)); TESTS+=("FAIL: $1"); echo -e "  ${RED}FAIL${NC} $1"; }

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [[ "$expected" == "$actual" ]]; then
        pass "$label (expected=$expected)"
    else
        fail "$label (expected=$expected, got=$actual)"
    fi
}

assert_ge() {
    local label="$1" min="$2" actual="$3"
    if [[ "$actual" -ge "$min" ]]; then
        pass "$label (>=$min, got=$actual)"
    else
        fail "$label (expected >=$min, got=$actual)"
    fi
}

assert_contains() {
    local label="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -qF -- "$needle"; then
        pass "$label"
    else
        fail "$label (missing: $needle)"
    fi
}

run_cascade() {
    local gate="$1" source="${2:-origin}" extra="${3:-}"
    GATE_NAME="$gate" bash "$CASCADE" --gate auto --source "$source" --dry-run $extra 2>&1
}

count_would_pull() {
    echo "$1" | grep -c "WOULD PULL" || echo "0"
}

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " WaterFall Gate-Sync Local Tests (manifest-driven)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── Test 1: Prerequisites ────────────────────────────────────────────────────

echo -e "${BLUE}[1] Prerequisites${NC}"

if [[ -f "$CASCADE" ]]; then
    pass "cascade-pull.sh exists"
else
    fail "cascade-pull.sh not found at $CASCADE"
    echo "Cannot continue without cascade-pull.sh"
    exit 1
fi

if [[ -f "$MANIFEST" ]]; then
    pass "ecosystem_manifest.toml exists"
else
    fail "ecosystem_manifest.toml not found"
fi

if command -v python3 >/dev/null 2>&1; then
    pass "python3 available"
else
    fail "python3 not found"
fi

echo ""

# ── Test 2: Gate identity via GATE_NAME env ──────────────────────────────────

echo -e "${BLUE}[2] Gate identity (GATE_NAME env)${NC}"

for gate in eastGate ironGate strandGate biomeGate southGate golgiBody; do
    output=$(run_cascade "$gate")
    detected=$(echo "$output" | grep "^Gate:" | awk '{print $2}' || echo "NONE")
    assert_eq "$gate identity" "$gate" "$detected"
done

echo ""

# ── Test 3: Gate profile repo counts (manifest-driven) ───────────────────────

echo -e "${BLUE}[3] Gate profile repo counts (from manifest)${NC}"

east_out=$(run_cascade "eastGate")
east_count=$(count_would_pull "$east_out")
assert_eq "eastGate repos (full superset)" "38" "$east_count"

iron_out=$(run_cascade "ironGate")
iron_count=$(count_would_pull "$iron_out")
assert_eq "ironGate repos" "22" "$iron_count"

strand_out=$(run_cascade "strandGate")
strand_count=$(count_would_pull "$strand_out")
assert_eq "strandGate repos" "24" "$strand_count"

biome_out=$(run_cascade "biomeGate")
biome_count=$(count_would_pull "$biome_out")
assert_eq "biomeGate repos" "19" "$biome_count"

south_out=$(run_cascade "southGate")
south_count=$(count_would_pull "$south_out")
assert_eq "southGate repos" "20" "$south_count"

golgi_out=$(run_cascade "golgiBody")
golgi_count=$(count_would_pull "$golgi_out")
assert_eq "golgiBody repos" "17" "$golgi_count"

echo ""

# ── Test 4: Profile content validation ───────────────────────────────────────

echo -e "${BLUE}[4] Profile content validation${NC}"

assert_contains "eastGate has primalSpring" "primalSpring" "$east_out"
assert_contains "eastGate has hotSpring" "hotSpring" "$east_out"
assert_contains "eastGate has helixVision" "helixVision" "$east_out"
assert_contains "eastGate has skunkBat" "skunkBat" "$east_out"

assert_contains "ironGate has healthSpring" "healthSpring" "$iron_out"
assert_contains "ironGate has ludoSpring" "ludoSpring" "$iron_out"
assert_contains "ironGate has skunkBat" "skunkBat" "$iron_out"

assert_contains "strandGate has hotSpring" "hotSpring" "$strand_out"
assert_contains "strandGate has helixVision" "helixVision" "$strand_out"
assert_contains "strandGate has initioChem" "initioChem" "$strand_out"
assert_contains "strandGate has blueFish" "blueFish" "$strand_out"
assert_contains "strandGate has lithoSpore" "lithoSpore" "$strand_out"
assert_contains "strandGate has wetSpring" "wetSpring" "$strand_out"

assert_contains "biomeGate has hotSpring" "hotSpring" "$biome_out"

assert_contains "southGate has wetSpring" "wetSpring" "$south_out"
assert_contains "southGate has neuralSpring" "neuralSpring" "$south_out"

echo ""

# ── Test 5: Source routing ───────────────────────────────────────────────────

echo -e "${BLUE}[5] Source routing${NC}"

origin_out=$(run_cascade "eastGate" "origin")
assert_contains "origin source in header" "Source:  origin" "$origin_out"

forgejo_out=$(run_cascade "eastGate" "forgejo")
assert_contains "forgejo source in header" "Source:  forgejo" "$forgejo_out"

auto_out=$(run_cascade "eastGate" "auto")
assert_contains "auto source in header" "Source:  auto" "$auto_out"

echo ""

# ── Test 6: Manifest consistency ─────────────────────────────────────────────

echo -e "${BLUE}[6] Manifest consistency${NC}"

manifest_version=$(echo "$east_out" | grep "Manifest:" | head -1)
assert_contains "manifest version in output" "v2" "$manifest_version"

repos_line=$(echo "$east_out" | grep "^Repos:" | awk '{print $2}')
assert_eq "repos count matches manifest" "38" "$repos_line"

echo ""

# ── Test 7: .gate identity file ──────────────────────────────────────────────

echo -e "${BLUE}[7] .gate identity file${NC}"

GATE_FILE="$ECO_ROOT/.gate"
GATE_BACKUP=""

if [[ -f "$GATE_FILE" ]]; then
    GATE_BACKUP=$(cat "$GATE_FILE")
fi

echo "testGateFile" > "$GATE_FILE"
gate_file_out=$(unset GATE_NAME; bash "$CASCADE" --gate auto --dry-run 2>&1 || true)

if echo "$gate_file_out" | grep -q "ERROR.*unknown gate"; then
    pass ".gate file read (testGateFile → unknown gate error as expected)"
else
    fail ".gate file should have been read"
fi

echo "eastGate" > "$GATE_FILE"
gate_file_east=$(unset GATE_NAME; bash "$CASCADE" --gate auto --source origin --dry-run 2>&1)
gate_file_detected=$(echo "$gate_file_east" | grep "^Gate:" | awk '{print $2}')
assert_eq ".gate file eastGate detection" "eastGate" "$gate_file_detected"

if [[ -n "$GATE_BACKUP" ]]; then
    echo "$GATE_BACKUP" > "$GATE_FILE"
else
    rm -f "$GATE_FILE"
fi

echo ""

# ── Test 8: Help output ──────────────────────────────────────────────────────

echo -e "${BLUE}[8] Help output${NC}"

help_out=$(bash "$CASCADE" --help 2>&1)
assert_contains "help has --gate" "--gate" "$help_out"
assert_contains "help has --source" "--source" "$help_out"
assert_contains "help has --clone-missing" "--clone-missing" "$help_out"
assert_contains "help has --check" "--check" "$help_out"
assert_contains "help has --parallel" "--parallel" "$help_out"
assert_contains "help has .gate file docs" ".gate" "$help_out"
assert_contains "help has waterFall domain" "waterFall" "$help_out"

echo ""

# ── Test 9: All gates in manifest are pullable ───────────────────────────────

echo -e "${BLUE}[9] All manifest gates dry-run successfully${NC}"

known_gates=$(python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib

with open('$MANIFEST', 'rb') as f:
    m = tomllib.load(f)
for g in sorted(m.get('gates', {}).keys()):
    print(g)
")

for gate in $known_gates; do
    gate_out=$(run_cascade "$gate" 2>&1)
    if echo "$gate_out" | grep -q "WOULD PULL"; then
        pass "$gate dry-run succeeds"
    else
        fail "$gate dry-run failed"
    fi
done

echo ""

# ── Summary ──────────────────────────────────────────────────────────────────

TOTAL=$((PASS + FAIL))
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e " Results: ${GREEN}$PASS PASS${NC} / ${RED}$FAIL FAIL${NC} / $TOTAL TOTAL"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "Failures:"
    for t in "${TESTS[@]}"; do
        [[ "$t" == FAIL* ]] && echo "  $t"
    done
    exit 1
fi

exit 0
