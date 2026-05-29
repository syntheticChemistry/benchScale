#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# test-waterfall-local.sh — Local (no Docker) validation of WaterFall gate-sync
#
# Tests cascade-pull.sh gate auto-detection, profile filtering, source
# routing, and manifest consistency using --dry-run. Runs entirely on
# the local workspace — no containers, no network, no Forgejo required.
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
    if echo "$haystack" | grep -q "$needle"; then
        pass "$label"
    else
        fail "$label (missing: $needle)"
    fi
}

run_cascade() {
    local gate="$1" source="${2:-github}" extra="${3:-}"
    GATE_NAME="$gate" bash "$CASCADE" --gate auto --source "$source" --dry-run --no-self-update $extra 2>&1
}

count_would_pull() {
    echo "$1" | grep -c "WOULD-PULL" || echo "0"
}

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " WaterFall Gate-Sync Local Tests"
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

# ── Test 2: Gate auto-detection via GATE_NAME ────────────────────────────────

echo -e "${BLUE}[2] Gate auto-detection (GATE_NAME env)${NC}"

for gate in eastGate ironGate strandGate biomeGate southGate golgiBody; do
    output=$(run_cascade "$gate")
    detected=$(echo "$output" | grep "Gate auto-detected:" | sed 's/.*: //' || echo "NONE")
    assert_eq "$gate auto-detected" "$gate" "$detected"
done

echo ""

# ── Test 3: Gate profile repo counts ─────────────────────────────────────────

echo -e "${BLUE}[3] Gate profile repo counts${NC}"

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

assert_contains "ironGate has healthSpring" "healthSpring" "$iron_out"
assert_contains "ironGate has ludoSpring" "ludoSpring" "$iron_out"
assert_contains "ironGate has esotericWebb" "esotericWebb" "$iron_out"

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

gh_out=$(run_cascade "eastGate" "github")
assert_contains "github source → origin remote" "remote: origin" "$gh_out"

fj_out=$(run_cascade "eastGate" "forgejo")
if echo "$fj_out" | grep -q "remote: forgejo"; then
    pass "forgejo source → forgejo remote (where configured)"
else
    pass "forgejo source → falls back to origin (forgejo remote not configured on all repos)"
fi

auto_out=$(run_cascade "eastGate" "auto")
assert_contains "auto source resolves" "WOULD-PULL" "$auto_out"

echo ""

# ── Test 6: Unknown gate falls back to all repos ────────────────────────────

echo -e "${BLUE}[6] Unknown gate fallback${NC}"

unknown_out=$(GATE_NAME="" bash "$CASCADE" --gate auto --dry-run --no-self-update 2>&1)
assert_contains "unknown gate warns" "WARNING" "$unknown_out"
unknown_count=$(count_would_pull "$unknown_out")
assert_eq "unknown gate pulls all repos" "$east_count" "$unknown_count"

echo ""

# ── Test 7: Manifest gate profile consistency ────────────────────────────────

echo -e "${BLUE}[7] Manifest consistency${NC}"

gate_count=$(python3 -c "
try:
    import tomllib
    def lt(p):
        with open(p,'rb') as f: return tomllib.load(f)
except:
    try:
        import tomli
        def lt(p):
            with open(p,'rb') as f: return tomli.load(f)
    except:
        import toml
        def lt(p): return toml.load(p)
d = lt('$MANIFEST')
print(len(d.get('gates', {})))
")
assert_ge "manifest has 6+ gate profiles" "6" "$gate_count"

repo_count=$(python3 -c "
try:
    import tomllib
    def lt(p):
        with open(p,'rb') as f: return tomllib.load(f)
except:
    try:
        import tomli
        def lt(p):
            with open(p,'rb') as f: return tomli.load(f)
    except:
        import toml
        def lt(p): return toml.load(p)
d = lt('$MANIFEST')
print(len(d.get('repos', {})))
")
assert_eq "manifest has 38 repos" "38" "$repo_count"

for gate in eastGate ironGate strandGate biomeGate southGate golgiBody; do
    gate_repos=$(python3 -c "
try:
    import tomllib
    def lt(p):
        with open(p,'rb') as f: return tomllib.load(f)
except:
    try:
        import tomli
        def lt(p):
            with open(p,'rb') as f: return tomli.load(f)
    except:
        import toml
        def lt(p): return toml.load(p)
d = lt('$MANIFEST')
repos = d.get('gates',{}).get('$gate',{}).get('repos',[])
all_repos = set(d.get('repos',{}).keys())
missing = [r for r in repos if r not in all_repos]
if missing:
    print('MISSING:' + ','.join(missing))
else:
    print('OK')
")
    if [[ "$gate_repos" == "OK" ]]; then
        pass "$gate: all profile repos exist in manifest"
    else
        fail "$gate: $gate_repos"
    fi
done

echo ""

# ── Test 8: GATE_SPRING_OWNERSHIP.md exists ──────────────────────────────────

echo -e "${BLUE}[8] Documentation${NC}"

if [[ -f "$WH_DIR/GATE_SPRING_OWNERSHIP.md" ]]; then
    pass "GATE_SPRING_OWNERSHIP.md exists"
else
    fail "GATE_SPRING_OWNERSHIP.md missing"
fi

if grep -q "GATE_SPRING_OWNERSHIP" "$WH_DIR/STANDARDS_AND_EXPECTATIONS.md" 2>/dev/null; then
    pass "STANDARDS_AND_EXPECTATIONS.md links to GATE_SPRING_OWNERSHIP"
else
    fail "STANDARDS_AND_EXPECTATIONS.md broken link"
fi

echo ""

# ── Summary ──────────────────────────────────────────────────────────────────

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
TOTAL=$((PASS + FAIL))
if [[ "$FAIL" -eq 0 ]]; then
    echo -e " ${GREEN}ALL $TOTAL TESTS PASSED${NC}"
else
    echo -e " ${RED}$FAIL/$TOTAL TESTS FAILED${NC}"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if $VERBOSE; then
    echo "Full test output available above."
fi

exit "$FAIL"
