# pixel-canvas

Standalone demo frontend for the **Distributed Pixel Canvas** OaaS tutorial.

Two files, no build step, no npm — open `index.html` in a browser.

---

## Quick start

```sh
# Serve locally (any static file server works)
cd tools/pixel-canvas
python3 -m http.server 3000
# Then open http://localhost:3000
```

Or open `index.html` directly from the file system (note: some browsers block
ES module imports from `file://` — use the server instead).

---

## URL parameters

All configuration is passed via URL query string.
If any required parameter is missing, the config form is shown instead.

### Audience mode (draw on your tile)

```
http://localhost:3000?mode=audience&gateway=<edge-gateway-url>&grid=<X-Y>
```

| Param     | Required | Description                                           |
|-----------|----------|-------------------------------------------------------|
| `mode`    | yes      | `audience`                                            |
| `gateway` | yes      | Edge gateway base URL, e.g. `http://edge.example.com:8080` |
| `grid`    | yes      | Tile position in mosaic, e.g. `0-0`, `3-2`           |

**Example:**
```
?mode=audience&gateway=http://localhost:8080&grid=0-0
```

### Presenter mode (full mosaic view)

```
http://localhost:3000?mode=presenter&gateway=<cloud-gateway-url>&cols=<N>&rows=<M>
```

| Param     | Required | Description                                          |
|-----------|----------|------------------------------------------------------|
| `mode`    | yes      | `presenter`                                          |
| `gateway` | yes      | Cloud gateway base URL                               |
| `cols`    | no       | Mosaic columns (default `4`)                         |
| `rows`    | no       | Mosaic rows (default `4`)                            |

**Example:**
```
?mode=presenter&gateway=http://localhost:8080&cols=4&rows=4
```

---

## OaaS collection config

Deploy a collection named `pixel-canvas` with per-entry state before using this frontend:

```json
{
  "name": "pixel-canvas",
  "partition_count": 1,
  "replica_count": 3,
  "shard_type": "mst",
  "options": {
    "mst_sync_interval": "1000"
  }
}
```

Canvas objects are created automatically on first write.
Each object ID encodes its tile position: `canvas-{col}-{row}`.
Each pixel is stored as an entry: key `"x:y"`, value is the CSS color string
(`#rrggbb`) UTF-8 encoded as base64.

---

## How it works

| Concern          | Approach                                                          |
|------------------|-------------------------------------------------------------------|
| Rendering        | HTML `<canvas>` element (32×32 pixels at 10px/cell)               |
| Drawing          | Pointer events (`pointerdown` + `pointermove`) — works on touch   |
| Writes           | Direct `PUT /api/class/pixel-canvas/0/objects/canvas-X-Y` with all pixels; debounced 300 ms |
| Reads / sync     | Poll every 2 s (audience) or 1 s (presenter mosaic)              |
| Entry encoding   | `btoa(color)` / `atob(data)`                                      |
| Presenter edit   | Click any tile → inline `AudienceCanvas` for that tile            |

---

## CORS

The gateway has `Access-Control-Allow-Origin: *` enabled (via `CorsLayer::permissive`
in `data-plane/oprc-gateway/src/handler/mod.rs`), so browser requests work directly.

If you're running an older gateway build without CORS, use a local proxy:

```sh
# Simple CORS proxy with npx
npx local-cors-proxy --proxyUrl http://your-gateway:8080 --port 8010
# Then set gateway=http://localhost:8010 in the URL
```

---

## Files

```
tools/pixel-canvas/
├── index.html   — page shell, config form, mode router
└── canvas.js    — ES module: fetchCanvas, saveCanvas, AudienceCanvas, PresenterMosaic
```
