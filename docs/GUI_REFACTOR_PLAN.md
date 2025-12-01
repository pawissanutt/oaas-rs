# GUI Architecture Refactoring Plan

## Goal
Simplify the GUI architecture by removing the "Fullstack" server component from `oprc-gui` and leveraging the existing Control Plane services (`oprc-pm` and `oprc-crm`) to serve the frontend and provide APIs.

## Architecture

### 1. Frontend (`oprc-gui`)
*   **Type**: Pure Client-Side Rendered (CSR) Dioxus WASM application.
*   **Responsibility**: Rendering UI, fetching data from REST APIs.
*   **Changes**:
    *   Remove `server` feature and all server-side logic (Zenoh sidecars, direct DB access).
    *   Replace `#[server]` functions with standard `reqwest` calls to `/api/v1/...`.
    *   Build output: Static files (`index.html`, `assets/`, `oprc-gui.wasm`, `oprc-gui.js`).

### 2. Package Manager (`oprc-pm`)
*   **Role**: User-facing Gateway & Static File Server.
*   **Responsibility**:
    *   Serve the static frontend files from a configured directory (e.g., `/ui` or root).
    *   Expose public REST APIs for the frontend (e.g., `/api/v1/packages`, `/api/v1/deployments`).
    *   **New**: Expose `/api/v1/topology` which proxies the request to `oprc-crm` via gRPC.

### 3. Cluster Resource Manager (`oprc-crm`)
*   **Role**: Infrastructure State Owner.
*   **Responsibility**:
    *   Manage Kubernetes resources.
    *   Connect to the Zenoh mesh.
    *   **New**: Expose a gRPC `TopologyService` that returns the mesh topology (Zenoh routers + K8s pods).

## Data Flow
1.  **User** opens browser to `http://pm-host:8080/`.
2.  **PM** serves the Dioxus WASM app.
3.  **Frontend** requests `GET /api/v1/topology`.
4.  **PM** receives request, calls `crm_client.get_topology()` (gRPC).
5.  **CRM** gathers data from Zenoh and returns `TopologySnapshot`.
6.  **PM** converts gRPC response to JSON and returns to Frontend.

## Implementation Steps
1.  [x] Define `TopologyService` gRPC contract in `commons/oprc-grpc`.
2.  [x] Implement `TopologyService` in `oprc-crm` (integrating Zenoh).
3.  [x] Update `oprc-pm` to consume `TopologyService` and expose REST endpoint (`/api/v1/topology`).
4.  [x] Refactor `oprc-gui` to remove server code and use REST client.
5.  [x] Configure `oprc-pm` to serve static files via `ServeDir` fallback.

## Status
✅ **COMPLETED** - All components have been refactored:
- CRM exposes gRPC `TopologyService` with Zenoh integration
- PM provides REST API `/api/v1/topology` and serves static files
- GUI is now pure CSR with `reqwest` API calls
