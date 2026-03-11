# Distributed Pixel Canvas — Conference Tutorial Design

A hands-on conference tutorial where attendees draw on personal canvases at the **edge**, which sync to a combined mosaic on the presenter's **cloud** display — demonstrating OaaS consistency models and network partition behavior.

---

## Concept

Each audience member draws on a small **32×32 pixel canvas** via their web browser. Each canvas is connected to one **OaaS object** hosted on the edge. The edge ODGM syncs state with the cloud ODGM. The presenter's screen displays a **combined mosaic** of all canvases, tiled in a configurable grid.

```
 AUDIENCE (Edge)                                PRESENTER (Cloud)
┌────────┐ ┌────────┐ ┌────────┐               ┌──────────────────────┐
│ 32×32  │ │ 32×32  │ │ 32×32  │               │ ┌──┬──┬──┬──┐        │
│ (0,0)  │ │ (1,0)  │ │ (2,0)  │               │ │  │  │  │  │        │
└───┬────┘ └───┬────┘ └───┬────┘ ◄──sync──►    │ ├──┼──┼──┼──┤        │
    │          │          │                    │ │  │  │  │  │ Mosaic │
    ▼          ▼          ▼                    │ ├──┼──┼──┼──┤        │
┌──────────────────────────────┐               │ │  │  │  │  │        │
│     OaaS Edge Gateway+ODGM   │◄──Raft/MST──► │ └──┴──┴──┴──┘        │
└──────────────────────────────┘               └──────────┬───────────┘
                                                          │
                                                ┌─────────▼──────────┐
                                                │ OaaS Cloud Gateway │
                                                │ + ODGM             │
                                                └────────────────────┘
```

### Key Demos

1. **Edge-to-cloud sync**: Audience draws → appears on presenter mosaic
2. **Cloud-to-edge sync**: Presenter draws on a canvas → audience sees it on their phone
3. **Network partition (MST)**: Break sync → both sides diverge → heal → LWW merges
4. **Network partition (Raft)**: Break sync → minority side blocks → heal → writes resume
5. **Sync interval tuning**: Slide `mst_sync_interval` to show latency-consistency tradeoff

---

## Data Model

### Object Identity = Grid Position

Each canvas object's ID encodes its position in the mosaic grid:

| Object ID | Meaning |
|---|---|
| `canvas-0-0` | Grid position (0,0) — top-left |
| `canvas-1-0` | Grid position (1,0) — second column, first row |
| `canvas-3-2` | Grid position (3,2) — fourth column, third row |

This makes mosaic rendering trivial: parse the object ID → place it at the right tile.

### Per-Entry Pixel Storage

Each pixel is a per-entry field on the object:

| Object ID | Entry Key | Value |
|---|---|---|
| `canvas-0-0` | `0:0` | `#FF0000` |
| `canvas-0-0` | `15:31` | `#00FF00` |
| `canvas-1-0` | `10:5` | `#0000FF` |
| `canvas-0-0` | `_meta` | `{"name":"Alice"}` |

The `_meta` entry stores user display name and any other metadata.

### Collection Config

```json
{
  "name": "pixel-canvas",
  "partition_count": 1,
  "replica_count": 3,
  "shard_type": "mst",
  "options": {
    "mst_sync_interval": "1000"
  },
  "invocations": {
    "fn_routes": {
      "paint": { "url": "wasm://pixel-canvas", "stateless": false }
    }
  }
}
```

Switch `shard_type` between `"raft"` and `"mst"` to change consistency.

---

## Frontend

**One web application**, two modes via URL parameters. Gateway URLs are configurable — no hardcoded endpoints.

### Audience Mode

```
URL: ?mode=audience&gateway=<edge-url>&grid=<x>-<y>
```

- **32×32 drawable canvas** with color picker (touch-friendly for phones)
- Sends `paint(x, y, color)` to the edge gateway on each brush stroke
- Polls `getCanvas()` periodically to see updates pushed from cloud (e.g., presenter drawing)
- The grid position `<x>-<y>` determines which object this user owns (`canvas-{x}-{y}`)

### Presenter Mode

```
URL: ?mode=presenter&gateway=<cloud-url>&cols=<N>&rows=<M>
```

- **Configurable grid**: `cols × rows` (e.g., `4×4`, `8×8`, `32×32`) determines mosaic size
- Reads all `canvas-*` objects from the cloud gateway and tiles them
- Auto-refreshes via polling
- **Presenter can draw** on any canvas tile by clicking it → sends update to cloud → syncs back to audience's edge
- **Control panel**:
  - Consistency model switch (Raft ↔ MST)
  - Network partition toggle (simulate edge-cloud link break)
  - Sync interval slider (`mst_sync_interval`)
  - Grid size config

---

## WASM Function

TypeScript via `@oaas/sdk`, compiled by `oprc-compiler`:

```typescript
import { service, method, OaaSObject } from "@oaas/sdk";

@service("PixelCanvas")
class Canvas extends OaaSObject {
  @method()
  async paint(x: number, y: number, color: string): Promise<void> {
    this.set(`${x}:${y}`, color);
  }

  @method({ stateless: true })
  async getCanvas(): Promise<Record<string, string>> {
    return this.getAll();
  }
}

export default Canvas;
```

> **Key point**: This exact code runs identically under Raft and MST. Consistency is an infrastructure concern, invisible to application logic.

### Server-Side Processing: Game of Life

A **stateless** WASM function runs Conway's Game of Life across the **entire mosaic** as a single global simulation. All canvas objects are stitched together into one large grid so cells at tile edges interact with neighbors on adjacent canvases.

#### Rules (with color)

Standard Conway's Game of Life:
- **Survival**: Alive cell with 2–3 alive neighbors → survives, keeps its color
- **Death**: Alive cell with <2 or >3 alive neighbors → dies (removed)
- **Birth**: Dead cell with exactly 3 alive neighbors → born with **average color** of the 3 parent neighbors (RGB channels averaged independently)

Color averaging creates organic color blending across the mosaic — two red neighbors and one blue neighbor produce a purple offspring.

#### Cross-Canvas Stitching

The function reads all `canvas-{x}-{y}` objects via cross-object proxy access, assembles a global `(cols×32) × (rows×32)` pixel grid, runs one GoL step, then writes back only changed pixels to each canvas:

```
 Canvas (0,0)          Canvas (1,0)
┌──────────────┐ ┌──────────────┐
│         ...AB│ │CD...         │
│         ...EF│ │GH...         │    ← Edge pixels A–H are
└──────────────┘ └──────────────┘      neighbors in the GoL grid
 Canvas (0,1)          Canvas (1,1)
┌──────────────┐ ┌──────────────┐
│         ...IJ│ │KL...         │
│         ...  │ │  ...         │
└──────────────┘ └──────────────┘
```

Cell F at `canvas-0-0 (31,1)` has neighbors including G at `canvas-1-0 (0,1)` and J at `canvas-0-1 (31,0)`.

#### Implementation

```typescript
@method({ stateless: true })
async golStep(cols: number, rows: number): Promise<{ births: number; deaths: number }> {
  // 1. Read all canvas objects via cross-object proxy
  for each canvas-{cx}-{cy}:
    const proxy = this.object(`pixel-canvas/0/canvas-${cx}-${cy}`);
    const all = await proxy.getAll();
    // map entries to global grid coordinates

  // 2. Compute next generation (standard GoL + color averaging)
  // 3. Write back only changed pixels (setMany for births/updates, delete for deaths)
  return { births, deaths };
}
```

> **Key point**: The function is **stateless** — it does not use `this.self`. Instead it reads and writes to **multiple** canvas objects via `this.object()`. This showcases OaaS cross-object access: one function orchestrating computation across many distributed objects.

#### Collection Config (updated)

```json
{
  "name": "pixel-canvas",
  "partition_count": 1,
  "replica_count": 3,
  "shard_type": "mst",
  "options": {
    "mst_sync_interval": "1000"
  },
  "invocations": {
    "fn_routes": {
      "paint": { "url": "wasm://pixel-canvas", "stateless": false },
      "paintBatch": { "url": "wasm://pixel-canvas", "stateless": false },
      "getCanvas": { "url": "wasm://pixel-canvas", "stateless": true },
      "golStep": { "url": "wasm://pixel-canvas", "stateless": true }
    }
  }
}
```

---

## Infrastructure

All pre-provisioned. Attendees only need a phone browser.

| Component | Edge Site | Cloud Site |
|---|---|---|
| ODGM | 2 nodes | 1 nodes |
| Gateway | 1 replica | 1 replica |
| WASM (PixelCanvas) | Deployed | Deployed |
| oprc-compiler | Shared | Shared |

Both sites share the same `PixelCanvas` collection definition. ODGM's replication handles sync:
- **Raft**: All nodes (edge + cloud) in one Raft group → strong consistency, cross-site latency
- **MST**: Independent writes per site, periodic anti-entropy sync → eventual consistency

### Storage

In-memory for simplicity (OaaS supports persistent backends but ephemeral is fine for a demo).

---

## Tutorial Flow (~75 min)

### 1. Intro (5 min)
- OaaS concept: stateful objects + co-located WASM compute
- Show empty mosaic on projector

### 2. Join & Draw — MST Fast Sync (10 min)
- Attendees scan QR code → audience mode, auto-assigned grid position
- Everyone draws → mosaic populates on projector in real time
- Point out: each pixel is one entry, each canvas is one object, sync is automatic

### 3. Explain the Code (10 min)
- Walk through the 10-line TypeScript WASM function
- Show how per-entry storage maps to pixels
- Show collection config — highlight `shard_type` field

### 4. Game of Life (10 min)
- Presenter triggers `golStep` — the entire mosaic evolves as one global Game of Life
- Audience drawings mutate, colors blend across canvas boundaries
- Run a few steps: watch patterns emerge, colors average across tile edges
- Explain: **one stateless function** reads/writes all canvas objects via cross-object proxy
- Key insight: server-side compute across distributed objects — no client coordination needed

### 5. Network Partition — MST Mode (15 min)
- **Enable partition**: break edge↔cloud sync
- Audience keeps drawing → their canvases diverge from the mosaic (mosaic freezes)
- Presenter draws on a canvas from cloud → audience doesn't see it
- **Heal partition**: everything syncs, mosaic updates, audience sees presenter's additions
- Discuss: LWW conflict resolution, anti-entropy, eventual consistency guarantees

### 6. Switch to Raft (10 min)
- Redeploy with `shard_type: "raft"`
- **Enable partition**: writes on the minority side **block/timeout**
- Attendees see: "I can't draw!" — that's the cost of strong consistency
- **Heal partition**: immediate resume
- Discuss: CAP theorem, when to choose strong vs eventual

### 7. Tune Sync Interval (5 min)
- Back to MST: slide `mst_sync_interval` from 5s → 500ms → 100ms
- Watch mosaic refresh speed change
- Discuss: bandwidth vs freshness tradeoff

### 8. Recap & Q&A (5 min)
- Takeaway: **same function, same object, consistency is a config knob**
- Server-side cross-object compute (GoL) — no client-side orchestration
- Real-world: edge retail, factory IoT, mobile-first apps, geo-distributed services

---

## What OaaS Uniquely Provides

| Concern | Without OaaS | With OaaS |
|---|---|---|
| Stateful compute | DB + service + glue | Object with co-located WASM |
| Per-pixel updates | Custom schema + ORM | Built-in per-entry storage |
| Edge-cloud sync | Custom CDC + conflict resolution | Built-in Raft/MST replication |
| Switch consistency | Rewrite replication layer | Change one config field |
| Deploy app logic | Container build + deploy pipeline | POST TypeScript → compile API |
| Cross-object compute | Distributed transactions + coordination | Stateless function with cross-object proxy |
| Scale out | Manual sharding | Partition-based, automatic |
