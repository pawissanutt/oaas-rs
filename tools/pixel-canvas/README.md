# Pixel Canvas — OaaS Tutorial Demo

Distributed pixel canvas demo for the OaaS conference tutorial. Attendees draw on personal 32×32 canvases at the edge, which sync to a combined mosaic on the presenter's cloud display.

## Project Structure

```
pixel-canvas/
├── package.json              # npm workspace root
├── tsconfig.base.json        # shared TypeScript config
├── frontend/                 # Vite + vanilla TypeScript web app
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.ts           # entry point
│       ├── config.ts         # URL param parsing + config form
│       ├── api.ts            # gateway REST client
│       ├── canvas.ts         # AudienceCanvas (32×32 drawable)
│       ├── mosaic.ts         # PresenterMosaic (N×M grid)
│       ├── render.ts         # shared canvas rendering
│       ├── types.ts          # type definitions + constants
│       └── style.css
└── wasm-guest/               # PixelCanvas WASM service
    ├── package.json
    ├── tsconfig.json
    ├── scripts/
    │   └── compile.js        # compile via oprc-compiler API
    └── src/
        └── canvas.ts         # @service("PixelCanvas") class
```

## Quick Start

### Install dependencies

```bash
cd tools/pixel-canvas
npm install
```

### Frontend development

```bash
# Start Vite dev server (proxies /api to gateway)
npm run dev

# With custom gateway URL
GATEWAY_URL=http://my-gateway:8080 npm run dev

# Production build
npm run build
```

Open `http://localhost:5173` in your browser. Use URL parameters to skip the config form:

- **Audience**: `?mode=audience&gateway=http://localhost:8080&grid=0-0`
- **Presenter**: `?mode=presenter&gateway=http://localhost:8080&cols=4&rows=4`

### WASM guest compilation

Requires `oprc-compiler` running locally:

```bash
# Start the compiler service (in another terminal)
cd tools/oprc-compiler
npm install && npm run dev

# Compile the PixelCanvas WASM guest
cd tools/pixel-canvas
npm run compile
# Output: wasm-guest/dist/pixel-canvas.wasm
```

### Type checking

```bash
npm run typecheck
```

## Architecture

### Frontend

Two modes controlled by URL parameters:

- **Audience mode**: 32×32 drawable canvas with color picker. Sends pixel updates to the edge gateway via PUT. Polls for remote changes every 2 seconds.
- **Presenter mode**: N×M grid of read-only canvas tiles, auto-refreshing every 1 second. Click a tile to open an inline editor.

The Vite dev server proxies `/api` requests to the gateway URL, avoiding CORS issues during development.

### WASM Guest

The `PixelCanvas` service uses `@oaas/sdk` decorators:

- `paint(x, y, color)` — set a single pixel
- `paintBatch(entries)` — set multiple pixels at once
- `getCanvas()` — return all pixels
- `clear()` — reset the canvas
- `setMeta(name)` — set user metadata

State is auto-persisted by the SDK — the `pixels` field (a `Record<string, string>` mapping `"x:y"` to CSS colors) is automatically loaded before and saved after each method call.

### Data Flow

```
Browser → PUT /api/class/pixel-canvas/0/objects/canvas-X-Y → Gateway → ODGM
Browser ← GET /api/class/pixel-canvas/0/objects/canvas-X-Y ← Gateway ← ODGM
```

Each pixel is stored as an entry on the object:
- Object ID: `canvas-{col}-{row}` (e.g., `canvas-0-0`)
- Entry key: `"x:y"` (e.g., `"15:31"`)
- Entry value: base64-encoded CSS color (e.g., `"#FF0000"`)

## E2E with Real Cluster

```bash
# 1. Create cluster
just create-cluster oaas-e2e

# 2. Deploy platform
just deploy

# 3. Create pixel-canvas collection
oprc class create --name pixel-canvas \
  --partition-count 1 --replica-count 3 \
  --shard-type mst \
  --option mst_sync_interval=1000

# 4. Deploy WASM guest (after compilation)
oprc package install wasm-guest/dist/pixel-canvas.wasm

# 5. Start frontend
GATEWAY_URL=http://<gateway-ip>:8080 npm run dev
```
