#!/usr/bin/env bash
# E2E test for pixel-canvas via the oprc dev server.
#
# Prerequisites:
#   - pixel-canvas WASM component compiled at tools/pixel-canvas/wasm-guest/dist/pixel-canvas.wasm
#   - oprc-cli built with dev-server feature
#
# The script:
#   1. Starts the dev server in the background
#   2. Waits for it to become healthy
#   3. Runs a series of HTTP tests against the gateway API
#   4. Shuts down the server
#   5. Reports results

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG="$SCRIPT_DIR/oaas-dev.yaml"
PORT=8089
BASE="http://localhost:$PORT"
CLS="pixel-canvas"
PID=0
SERVER_PID=""
PASS=0
FAIL=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

cleanup() {
    if [[ -n "$SERVER_PID" ]]; then
        echo ""
        echo "Shutting down dev server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

assert_eq() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"
    if [[ "$expected" == "$actual" ]]; then
        echo -e "  ${GREEN}PASS${NC}: $test_name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $test_name"
        echo "    expected: $expected"
        echo "    actual:   $actual"
        FAIL=$((FAIL + 1))
    fi
}

assert_contains() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"
    if echo "$actual" | grep -qF "$expected"; then
        echo -e "  ${GREEN}PASS${NC}: $test_name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $test_name"
        echo "    expected to contain: $expected"
        echo "    actual: $actual"
        FAIL=$((FAIL + 1))
    fi
}

assert_http_status() {
    local test_name="$1"
    local expected_status="$2"
    local method="$3"
    local url="$4"
    shift 4
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X "$method" "$url" "$@")
    assert_eq "$test_name" "$expected_status" "$status"
}

# ── Build ──────────────────────────────────────────────────────────────
echo -e "${YELLOW}=== Pixel Canvas E2E Test ===${NC}"
echo ""

# Check WASM module exists
WASM_PATH="$REPO_ROOT/tools/pixel-canvas/wasm-guest/dist/pixel-canvas.wasm"
if [[ ! -f "$WASM_PATH" ]]; then
    echo -e "${RED}ERROR: WASM module not found at $WASM_PATH${NC}"
    echo "  Run:  cd tools/pixel-canvas/wasm-guest && npm run compile"
    exit 1
fi

echo "Building oprc-cli with dev-server feature..."
cargo build -p oprc-cli --features dev-server -q
CLI_BIN="$REPO_ROOT/target/debug/oprc-cli"

# ── Start Server ───────────────────────────────────────────────────────
echo "Starting dev server on port $PORT..."
cd "$REPO_ROOT"
RUST_LOG=info "$CLI_BIN" dev serve --config "$CONFIG" --port "$PORT" &
SERVER_PID=$!

# Wait for the server to become healthy
echo -n "Waiting for server"
for i in $(seq 1 90); do
    if curl -sf "$BASE/healthz" > /dev/null 2>&1; then
        echo " ready!"
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo ""
        echo -e "${RED}ERROR: Dev server exited unexpectedly${NC}"
        SERVER_PID=""
        exit 1
    fi
    echo -n "."
    sleep 1
done

if ! curl -sf "$BASE/healthz" > /dev/null 2>&1; then
    echo ""
    echo -e "${RED}ERROR: Dev server did not become healthy in 90s${NC}"
    exit 1
fi

# ── Test: Health Endpoints ─────────────────────────────────────────────
echo ""
echo -e "${YELLOW}--- Health Endpoints ---${NC}"
assert_http_status "GET /healthz returns 200" "200" GET "$BASE/healthz"
assert_http_status "GET /readyz returns 200"  "200" GET "$BASE/readyz"

# Wait for WASM shard to be fully initialized
# The shard needs time for WASM compilation after the server starts accepting connections
echo ""
echo -n "Waiting for shard readiness"
for i in $(seq 1 60); do
    STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
        "$BASE/api/class/$CLS/$PID/objects/__probe__" \
        -H "Content-Type: application/json" \
        -d '{"entries":{}}')
    if [[ "$STATUS" == "200" ]]; then
        echo " ready!"
        # Clean up probe object
        curl -s -X DELETE "$BASE/api/class/$CLS/$PID/objects/__probe__" > /dev/null 2>&1 || true
        break
    fi
    echo -n "."
    sleep 1
done
if [[ "$STATUS" != "200" ]]; then
    echo ""
    echo -e "${RED}ERROR: Shard not ready after 60s (last status: $STATUS)${NC}"
    exit 1
fi

# ── Test: Stub PM API ─────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}--- Stub PM API ---${NC}"
DEPLOYMENTS=$(curl -sf "$BASE/api/v1/deployments")
assert_contains "deployments contains pixel-canvas" "pixel-canvas" "$DEPLOYMENTS"
PACKAGES=$(curl -sf "$BASE/api/v1/packages")
assert_contains "packages contains pixel-canvas" "pixel-canvas" "$PACKAGES"
ENVS=$(curl -sf "$BASE/api/v1/envs")
assert_eq "envs returns empty array" "[]" "$ENVS"

# ── Test: Object CRUD ─────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}--- Object CRUD ---${NC}"

# PUT an object with empty entries
assert_http_status "PUT canvas-0-0" "200" PUT \
    "$BASE/api/class/$CLS/$PID/objects/canvas-0-0" \
    -H "Content-Type: application/json" \
    -d '{"entries":{}}'

# GET the object back as JSON
GET_RESULT=$(curl -sf "$BASE/api/class/$CLS/$PID/objects/canvas-0-0" \
    -H "Accept: application/json")
assert_contains "GET canvas-0-0 has metadata" "canvas-0-0" "$GET_RESULT"

# PUT a second canvas
assert_http_status "PUT canvas-1-0" "200" PUT \
    "$BASE/api/class/$CLS/$PID/objects/canvas-1-0" \
    -H "Content-Type: application/json" \
    -d '{"entries":{}}'

# LIST objects
LIST_RESULT=$(curl -sf "$BASE/api/class/$CLS/$PID/objects" \
    -H "Accept: application/json")
assert_contains "LIST contains canvas-0-0" "canvas-0-0" "$LIST_RESULT"
assert_contains "LIST contains canvas-1-0" "canvas-1-0" "$LIST_RESULT"

# ── Test: WASM Function Invocation ────────────────────────────────────
echo ""
echo -e "${YELLOW}--- WASM Function Invocation ---${NC}"

# Invoke paint(x=15, y=20, color="#AABBCC") on canvas-0-0
PAINT_RESULT=$(curl -s -w "\n%{http_code}" \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/paint" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{"x": 15, "y": 20, "color": "#AABBCC"}')
PAINT_STATUS=$(echo "$PAINT_RESULT" | tail -1)
assert_eq "invoke paint returns 200" "200" "$PAINT_STATUS"

# Verify the pixel was written via getCanvas
GET_CANVAS_RESULT=$(curl -s -w "\n%{http_code}" \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/getCanvas" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{}')
GET_CANVAS_STATUS=$(echo "$GET_CANVAS_RESULT" | tail -1)
GET_CANVAS_BODY=$(echo "$GET_CANVAS_RESULT" | sed '$d')
assert_eq "invoke getCanvas returns 200" "200" "$GET_CANVAS_STATUS"
assert_contains "getCanvas has pixel 15:20" "15:20" "$GET_CANVAS_BODY"
assert_contains "getCanvas has color #AABBCC" "#AABBCC" "$GET_CANVAS_BODY"

# Invoke paintBatch on canvas-1-0 (argument is the direct Record<string, string>)
BATCH_RESULT=$(curl -s -w "\n%{http_code}" \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-1-0/invokes/paintBatch" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{"10:10": "#FF0000", "11:11": "#00FF00"}')
BATCH_STATUS=$(echo "$BATCH_RESULT" | tail -1)
assert_eq "invoke paintBatch returns 200" "200" "$BATCH_STATUS"

# Verify batch paint via getCanvas
GET_BATCH_CANVAS=$(curl -s \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-1-0/invokes/getCanvas" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{}')
assert_contains "canvas-1-0 has batch pixel 10:10" "10:10" "$GET_BATCH_CANVAS"
assert_contains "canvas-1-0 has batch pixel 11:11" "11:11" "$GET_BATCH_CANVAS"

# Invoke setMeta on canvas-0-0
META_RESULT=$(curl -s -w "\n%{http_code}" \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/setMeta" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{"name": "Alice"}')
META_STATUS=$(echo "$META_RESULT" | tail -1)
assert_eq "invoke setMeta returns 200" "200" "$META_STATUS"

# Verify meta was set via GET (the _meta entry should appear in entries)
GET_AFTER_META=$(curl -sf "$BASE/api/class/$CLS/$PID/objects/canvas-0-0" \
    -H "Accept: application/json")
assert_contains "canvas-0-0 has _meta entry" "_meta" "$GET_AFTER_META"

# ── Test: Game of Life (cross-object) ─────────────────────────────────
echo ""
echo -e "${YELLOW}--- Game of Life (Cross-Object Function) ---${NC}"

# Clear canvas-0-0 first, then set up a blinker pattern
curl -s -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/clear" \
    -H "Content-Type: application/json" -d '{}' > /dev/null 2>&1 || true

# Set up a blinker pattern on canvas-0-0 (3 vertical alive cells)
# Blinker: (15,14), (15,15), (15,16) → should rotate to horizontal
for coord in '{"x":15,"y":14,"color":"#FF0000"}' '{"x":15,"y":15,"color":"#FF0000"}' '{"x":15,"y":16,"color":"#FF0000"}'; do
    curl -s -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/paint" \
        -H "Content-Type: application/json" \
        -d "$coord" > /dev/null
done

# Invoke golStep (cols=1, rows=1 — just one canvas tile for simplicity)
GOL_RESULT=$(curl -s -w "\n%{http_code}" \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/golStep" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{"cols": 1, "rows": 1}')
GOL_STATUS=$(echo "$GOL_RESULT" | tail -1)
GOL_BODY=$(echo "$GOL_RESULT" | sed '$d')
assert_eq "invoke golStep returns 200" "200" "$GOL_STATUS"
assert_contains "golStep returns births count" "births" "$GOL_BODY"
assert_contains "golStep returns deaths count" "deaths" "$GOL_BODY"

# After one GoL step, the blinker should have rotated.
# Verify the canvas changed via getCanvas
GET_AFTER_GOL=$(curl -s \
    -X POST "$BASE/api/class/$CLS/$PID/objects/canvas-0-0/invokes/getCanvas" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{}')
# The horizontal blinker cells should now exist: (14,15) and (16,15)
assert_contains "GoL created cell at 14:15" "14:15" "$GET_AFTER_GOL"
assert_contains "GoL created cell at 16:15" "16:15" "$GET_AFTER_GOL"

# ── Test: /api/gateway prefix (frontend compatibility) ────────────────
echo ""
echo -e "${YELLOW}--- Frontend Gateway Prefix ---${NC}"

# The frontend uses /api/gateway/api/class/... as prefix
GATEWAY_PROXY_RESULT=$(curl -sf "$BASE/api/gateway/api/class/$CLS/$PID/objects/canvas-0-0" \
    -H "Accept: application/json")
assert_contains "gateway proxy returns canvas-0-0" "canvas-0-0" "$GATEWAY_PROXY_RESULT"

# ── Test: DELETE ──────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}--- Object Delete ---${NC}"
assert_http_status "DELETE canvas-1-0" "204" DELETE \
    "$BASE/api/class/$CLS/$PID/objects/canvas-1-0"

# ── Summary ────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}=== Results ===${NC}"
TOTAL=$((PASS + FAIL))
echo -e "  Total: $TOTAL  ${GREEN}Passed: $PASS${NC}  ${RED}Failed: $FAIL${NC}"

if [[ "$FAIL" -gt 0 ]]; then
    echo ""
    echo -e "${RED}SOME TESTS FAILED${NC}"
    exit 1
else
    echo ""
    echo -e "${GREEN}ALL TESTS PASSED${NC}"
    exit 0
fi
