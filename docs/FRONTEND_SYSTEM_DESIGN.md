# OaaS‑RS Frontend System Design (High‑Level)

This document describes the frontend system architecture for the OaaS‑RS Console at a conceptual level. It focuses on overall structure, responsibilities, and key data flows. No code, API shapes, or protocol details are included.

## 1) Purpose & Scope
- Provide a modern web console for developers, operators, and SREs to manage classes, objects/functions, deployments, and to visualize the Zenoh network topology.
- Align with OaaS‑RS conventions (env‑first config, tracing, health endpoints) and reuse shared crates/types where appropriate in implementation.
- Rendering model: Client‑Side Rendering (CSR) only. No server‑side rendering (SSR).
- Out of scope: Detailed endpoint contracts, low‑level protocol choices, implementation code.

## 2) Architectural Overview

### 2.1 High‑Level Components
- Frontend Client (CSR):
  - Sole rendering model: Client‑Side Rendering using Dioxus + Tailwind.
  - Fetches data for objects, invocations, deployments, metrics, and topology from server‑exposed integrations.
  - Manages local UI state (forms, selections) and interactive visualizations (e.g., topology graph); no client‑side data caching in MVP.
- Frontend Static/Helper Server:
  - Serves static assets (CSS/JS/fonts) and offers thin integration endpoints (e.g., topology snapshot/stream, consolidated health).
  - Centralizes configuration (environment variables) and observability (tracing/metrics) for the web UI.
- Topology Aggregator:
  - Background task that reads Zenoh admin space (when enabled) and maintains a current network snapshot.
  - Publishes periodic updates or diffs to clients via a streaming channel exposed by the server.

### 2.2 Rendering Strategy (Decision)
- Chosen model: CSR‑only for simplicity and faster developer iteration.
- LiveView‑style server‑driven UIs are not selected due to the highly interactive Topology canvas and client‑side graphing needs.

### 2.3 Deployment Topologies
- Standalone Console Service: runs as an independent deployment, reachable via an ingress.
- Co‑located with Gateway (optional): console served via the same ingress domain as Gateway; useful for simplified dev.
- Dev Mode: hot reload for UI; mock data sources when other services are unavailable.

### 2.4 Configuration (Env‑First)
- UI server address/port and log level.
- External services base URLs (e.g., Gateway, PM/CRM) for integration.
- Zenoh session config and a toggle to enable admin space (for topology).
- Feature flags for optional pages (Events, Metrics) in early phases.

## 3) Responsibilities & Boundaries
- Server‑Side Rendering
  - Produces initial HTML for all routes (consistent shell + per‑page content).
  - Injects environment/context into the page for client bootstrap.
- Static Asset Delivery
  - Delivers precompiled Tailwind CSS and minimal client runtime.
  - Uses far‑future caching with content hashing; short cache for HTML.
- Integration Adapters (Thin)
  - Provide minimal server endpoints for data aggregation that benefits from server proximity (e.g., topology snapshots/streams, consolidated health check).
  - Defer domain logic to existing OaaS‑RS components; avoid duplicating backend behavior.
- Security & Access Control
  - Support pluggable authentication strategies (dev mock → Single‑Sign‑On later).
  - Enforce basic authorization at the server envelope where applicable (page/feature gating).

## 4) Key Data Flows (Conceptual)

### 4.1 Page Rendering
1. Browser requests a page route (e.g., Deployments or Objects).
2. Server runs SSR for the route and returns HTML + critical CSS/JS.
3. Client hydrates; subsequent interactions perform lightweight fetches.

### 4.2 Object View
1. User provides Class, Partition, and Object ID.
2. Client requests object data via the designated integration path.
3. Server forwards/aggregates as necessary; returns normalized JSON for the view.
4. Client renders metadata + state; shows validation and error states on failures.

### 4.3 Function/Object Invocation
1. User configures target and payload; submits.
2. Client sends the request; server routes to the appropriate integration.
3. Response is normalized for display (status, content‑type, size, latency, body preview).
4. Client stores local history of invocations (timestamps, results).

### 4.4 Deployments & ClassRuntime Summary
1. Client requests a summary listing and/or details for a deployment.
2. Server fetches and composes human‑readable fields (status, conditions, desired vs observed generation, feature flags).
3. Client displays list cards and a detailed page with a conditions panel.

### 4.5 Topology (Zenoh Admin Space)
1. Topology Aggregator subscribes/queries admin space (when enabled) to build a live network graph.
2. Aggregator computes a snapshot and diffs on changes (nodes/links add/remove/change state).
3. Server exposes a read endpoint for snapshots and a streaming endpoint for live updates.
4. Client subscribes; renders the graph with pan/zoom and filters; debounces rapid changes for stability.

## 5) High‑Level Diagrams (ASCII)

### 5.1 Component View
```
[ Browser ]
  │  initial HTML/CSS/JS (CSR)
     ▼
[ Frontend Static/Helper Server ]──serves static──▶[ CDN/Cache ]
     │        │
     │        ├─ health/consolidation (light)
     │        ├─ topology snapshot/stream
     │        └─ env/config injection
     │
     ├────────▶ [ Gateway ]     (data plane access)
     ├────────▶ [ PM / CRM ]    (deployment/class runtime summaries)
     └────────▶ [ Zenoh Admin Space ] (topology source)
```

### 5.2 Topology Flow
```
[ Zenoh Admin Space ] → [ Topology Aggregator ] → [ Server Stream ] → [ Client Graph ]
                             ▲                         │
                             └──── snapshot read ──────┘
```

## 6) Security (High‑Level)
- Authentication: start with development‑mode mock; plan for SSO integration in later phases.
- Authorization: feature/page‑level gating, with the assumption that backend services enforce domain‑level access control.
- Transport: TLS termination at ingress/edge.

## 7) Performance & Resilience Goals
- Fast interactive readiness via CSR.
- No client‑side data caching in MVP: each view fetches fresh data; provide explicit Refresh actions where helpful. Caching can be introduced later.
- Graceful degradation: when optional integrations are unavailable (e.g., admin space), topology page downgrades to an information panel.
- Backoff and retry surfaces left to user interaction (explicit retry), not hidden automatic loops.

## 8) Observability & Operations (UI Server)
- Tracing for server request handling (route name, latency, errors).
- Counters for topology updates, connected clients, and stream errors.
- Health endpoints for liveness/readiness of the UI server.
- Config surfacing: a read‑only page (Settings) indicating which integrations are enabled.

## 9) Configuration Surfaces
- Admin space toggle: enables topology features.
- External base URLs or service discovery hints for data/control plane integrations.
- Feature flags for pages (Events, Metrics) and preview features (e.g., experimental topology filters).

## 10) Deployment & Environments
- Dev: single binary with file‑watch rebuild for UI; permissive CORS/headers if necessary.
- Staging/Prod: built assets versioned, cached via CDN; server behind ingress with auth in front (IdP/SSO gateway).
- Rollout: blue/green or canary at the ingress level; UI server is stateless.

## 11) Risks & Mitigations (Selected)
- Topology scale/volatility → debounce updates; send diffs not full snapshots; cap graph size with paging/grouping.
- Inconsistent deployment metadata → tolerant rendering with placeholders; surface what’s available.
- Client hydration cost → ship only essential code for the landing route; lazy load heavy pages (e.g., topology).

## 12) Phased Delivery
- Phase 1 (MVP): SSR shell, Objects/Invoke views, Deployments read‑only summary, topology snapshot.
- Phase 2: Object update/delete, richer Deployments detail (conditions), basic metrics.
- Phase 3: Live topology stream with diffing; invocation presets; refined accessibility.
- Phase 4: Advanced observability, SSO integration, performance hardening.

## 13) Out of Scope (for this document)
- Concrete API definitions, request/response schemas.
- Specific framework code, server choices, or library versions.
- Non‑UI service architecture or data model internals.
