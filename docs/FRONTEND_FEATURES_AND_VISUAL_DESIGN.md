# OaaS‑RS Frontend: Features, Requirements, and Visual Design

This document defines the product features, user requirements, and visual/UX design of the OaaS‑RS frontend application. It intentionally avoids system design and API contracts.

## Scope and Intent
- Build a full‑stack Dioxus application styled with Tailwind, using a CSR‑first approach.
- Server‑side rendering (SSR) is optional and not required; SEO is not a goal for this console.
- Provide a web console for developers and operators to manage classes, objects, invocations, deployments, and to visualize the Zenoh network topology.
- Out of scope: backend/system architecture, protocol choices, API shapes or endpoints.

### Rendering Approach (High‑Level)
- Primary: Client‑Side Rendering (CSR) for simplicity and faster iteration.
- Optional: SSR for the shell/pages if needed for perceived performance; can be added later without changing the visual design.
- Heavy interactive areas (e.g., Topology graph) are client‑rendered.

## Personas
- Platform Operator: monitors cluster and network health; validates deployments; inspects replication and routing topology.
- Application Developer: deploys packages/functions, invokes functions and object methods, inspects object state and events.
- SRE/Performance Analyst: reviews latency, error trends, resource health indicators.

## Goals
- Make object/function operations intuitive and fast to execute.
- Present clear, live health status and topology at a glance.
- Provide consistent, accessible, and responsive UI across devices.

## Core Features (User‑Facing)

### 1) Global Health and Status
- Header status showing Gateway readiness and general cluster health.
- Quick indicators: healthy/degraded/unavailable; last updated timestamp.

### 2) Classes & Objects
- Class/Partition browser: select a class and partition; fetch a specific object by id.
- Object detail view: metadata, state viewer, recent activity summary.
- Object actions: invoke method (form), update state, delete object.

### 3) Function Invocation
- Dedicated page for invoking class functions and object methods.
- Payload editor with JSON mode and raw text mode.
- Response viewer with auto content‑type detection and pretty JSON.
- Minimal local history of recent invocations.

### 4) Deployments (Read‑Only to start)
- List of deployments with basic status, replicas (if available), and feature flags presence (e.g., HPA, ODGM integration) based on available metadata.
- Detail page with rollout indicators and notes/links to source artifacts.
 - ClassRuntime integration (high‑level): Surface ClassRuntime name/namespace, conditions (Ready/Degraded), observed generation vs desired, referenced class/functions, and enabled features. No API details are shown here—only human‑readable summaries.

### 5) Events & Activity (Later)
- Tail a stream of object/class events with filters (type, class, partition, time window).
- Virtualized list for performance; ability to pause/resume live updates.

### 6) Metrics Overview (Later)
- Summary cards: request rate, latency percentile snapshot, error rate.
- Time range presets with basic charts.

### 7) Zenoh Network Topology
- Interactive graph view of peers/routers derived from Zenoh admin space.
- Visual indicators for node role/state; links for connectivity.
- Live updates (incremental) with lightweight animations.

## Requirements

### Functional Requirements
- Navigation: Sidebar with sections (Deployments, Classes, Objects, Invoke, Events, Topology, Metrics, Settings).
- Search/Select: Quick selectors for Class and Partition; object ID input with validation rules.
- Content Negotiation UX: JSON‑first display; show raw when non‑JSON.
- Invocations: Support free‑form payload entry; show response content‑type and size; store local history.
- Object Operations: View → Update → Delete with confirmation and undo affordances where feasible.
- Topology: Display nodes and links; allow zoom/pan; filter by role (peer/router); show last refresh time.
- Feedback: Toasts for success/error; inline validation errors; loading indicators and skeletons.

### Non‑Functional Requirements
- Accessibility: WCAG 2.1 AA goals; keyboard navigation across primary flows; focus ring visibility.
- Responsiveness: Optimized layouts for desktop first; usable on tablet; mobile read‑only acceptable for MVP.
- Performance: Initial content paint < 2s on typical dev laptop; interactive topology up to medium graphs.
- Internationalization: Copy and date/time formatting centralized; English only for MVP, extensible later.
- Reliability: Graceful handling of timeouts; clear error states; retry affordance only where user‑driven.
- Observability (UI): Minimal client‑side diagnostics panel (hidden) for troubleshooting (e.g., last error).

### UX Patterns & Validation
- Object ID validation mirrors backend rules (length ≤ 160; allowed chars: a‑z, 0‑9, . _ : -). 
- Destructive actions require confirmation; optional typed confirmation for DELETE.
- Empty states with clear guidance (e.g., “Enter Class, Partition, and Object ID to view.”).
- Copyable code blocks and IDs; title and aria‑labels for assistive tech.

## Information Architecture & Navigation
- Primary Navigation (Sidebar):
  - Deployments
  - Classes (browse)
  - Objects (direct lookup)
  - Invoke
  - Events (later)
  - Topology
  - Metrics (later)
  - Settings
- Secondary Navigation (Tabs or Sub‑nav inside pages): Details | Activity | JSON | Raw
- Global Search (later): quick jump to Class/Partition/Object by ID.

## Visual Design System (Tailwind)

### Principles
- Clarity over ornament; surface state and action affordances prominently.
- Information density with breathing room; consistent spacing and alignment.
- Motion for feedback only; subtle, not distracting.

### Color
- Base palette using Tailwind scales:
  - Primary: Indigo (interactive, emphasis)
  - Accent: Emerald (positive state)
  - Warning: Amber (attention)
  - Danger: Red (destructive, critical)
  - Info: Sky (informational chips)
  - Neutrals: Slate/Gray (backgrounds, borders, text)
- Status badges: success/neutral/warn/danger variants in light and dark.
- Dark mode supported with `dark:` variants and sufficient contrast.

### Typography
- Sans: Inter (UI copy);
- Mono: JetBrains Mono (payloads, JSON, IDs);
- Type scale: 12/14/16/20/24/32 for body→display.

### Spacing & Layout
- Spacing scale: Tailwind default with a tighter `2` step available.
- Containers: max‑w‑7xl content width; responsive gutters.
- Radius: md (6px), lg (10px) for cards and inputs.

### Components (Overview)
- Buttons: primary/secondary/tertiary; loading and disabled states.
- Inputs: text, number, select, textarea (code‑like for payloads), toggle.
- Cards & Panels: headers with actions; subdued surfaces.
- Tables & Lists: zebra rows; sticky headers on long lists.
- Badges & Chips: status and filters.
- Toasts: top‑right stack; auto‑dismiss with manual close.
- Modals/Drawers: confirm destructive actions; keyboard accessible.
- Code/JSON Viewer: pretty‑print, copy, collapse sections.
- Graph Canvas (Topology): force‑like layout; zoom/pan; legend overlay.

### Interaction Details
- Hover/focus: clear focus ring; hover color shifts within palette; pressed states.
- Loading: skeletons for panes; spinners for short actions.
- Errors: inline messages near fields; page‑level banner for critical failures.

## Page Blueprints (High‑Level)
- Deployments: grid/list, status badge, filter by status.
- Classes: selector controls, helper text, recent classes shortcut.
- Objects: ID input, fetch action, metadata + state panels.
- Invoke: request editor, content‑type switch, response viewer, history.
- Events (later): filters bar, virtualized stream, pause/resume.
- Topology: graph canvas, node list sidebar, filters and legend.
- Metrics (later): summary cards, quick trends.
- Settings: theme toggle, date/time format, units.

## Roadmap (No Technical Detail)
- Phase 1 (MVP): Health banner, Classes/Objects (view), Invoke, Topology (snapshot), light/dark theme, basic toasts.
- Phase 2: Object update/delete, Deployments (read‑only), basic metrics summary, Events (initial view).
- Phase 3: Live Topology updates, richer charts, saved invocation presets, improved accessibility testing.
- Phase 4: Advanced observability pages, localization groundwork, polished visuals.

## Edge Cases & UX Resilience
- Timeouts and transient failures: show retry affordance and guidance text.
- Large responses: truncated preview with download option.
- Missing metadata on object: clear not‑found state with next‑steps.
- Content that isn’t JSON: present raw with copy/download; avoid broken previews.
- Topology churn: debounce visual updates; show “Live” indicator with last update time.

## Content & Tone
- Plain language; action‑oriented labels (Invoke, Update, Delete).
- Concise error copy; prefer suggestions (“Try again”, “Check ID format”).
- Tooltips for advanced concepts; avoid jargon in primary UI.

## Open Questions
- Which deployment status signals are available for read‑only cards in MVP?
- Minimum/maximum expected graph size for topology view?
- Are there branding constraints (logo, colors) to incorporate into the theme?

## Visual Drafts (ASCII)

The following wireframes are high‑level sketches of key screens and the global layout. They illustrate structure and interactions only; not a pixel‑perfect design.

### Global Layout (Header + Sidebar + Content)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  OaaS‑RS Console                      Health: ● OK        Theme: ◑ Dark ☐    │
│  Search: [ Class … ] [ Partition … ] [ Object ID … ] (⌕)                      │
├──────────────────────────────────────────────────────────────────────────────┤
│  ■ Deployments                                                              │
│  ■ Classes                                                                  │
│  ■ Objects                                                                  │
│  ■ Invoke                                                                   │
│  ■ Events (later)                                                           │
│  ■ Topology                                                                 │
│  ■ Metrics (later)                                                          │
│  ■ Settings                                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [ Content Area ]                                                            │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
Legend: ● OK (green), ● ! (amber), ● × (red)
```

### Objects – Lookup and Details

```
┌──────────────── Objects ────────────────┐           ┌───────── Actions ───────┐
│ Class     [ my.class      ▾ ]          │           │  ( ) Invoke method      │
│ Partition [    0          ▾ ]          │           │  ( ) Update state       │
│ Object ID [ user-123               ] [Fetch]       │  ( ) Delete object      │
├─────────────────────────────────────────┤           │                         │
│ Status: ● Found   Last Updated: 12:03  │           │  [Perform Action]       │
├────────────── Metadata ────────────────┤           └───────────────────────────┘
│ cls_id: my.class   partition: 0        │
│ object_id_str: "user-123"              │
│ version: 42                             │
├──────────────── State (JSON) ──────────┤  [JSON ▾]  [Copy]  [Download]
│ {                                       │
│   "name": "Alice",                      │
│   "tier": "pro"                         │
│ }                                       │
└─────────────────────────────────────────┘
Validation: object id ≤ 160 chars; allowed: a‑z 0‑9 . _ : -
```

### Invoke – Function/Object Invocation

```
┌──────────────── Invoke ────────────────┐      ┌────────── Result ────────────┐
│ Target:  (●) Class Function  ( ) Object Method │ Status: 200 OK               │
│ Class     [ my.class     ▾ ]          │      │ Duration: 12.7 ms            │
│ Partition [   0          ▾ ]          │      │ Content‑Type: application/json│
│ Function  [ gen_greeting        ]     │      ├───────────────────────────────┤
│ Object ID [ disabled when Class Fn ]  │      │ { "message": "hello" }       │
│ Payload   [ JSON ▾ ]                  │      │ [Copy] [Save to History]      │
│ ┌───────────────────────────────────┐ │      └───────────────────────────────┘
│ │ { "name": "Alice" }              │ │
│ └───────────────────────────────────┘ │
│ Content‑Type [ application/json ▾ ]    │  Accept [ application/json ▾ ]
│ [ Invoke ]  [ Clear ]  [ Load Sample ] │
└────────────────────────────────────────┘
History (local): recent invocations with timestamp and status
```

### Deployments – List and Summary

```
┌──────────────── Deployments ──────────────── Filter: [ All ▾ ] [ Search … ] ┐
│ ┌──────────────────────────┐  ┌──────────────────────────┐  ┌───────────────┐ │
│ │ name: auth‑service      │  │ name: billing‑fn         │  │ name: router  │ │
│ │ status: ● OK            │  │ status: ● ! (degraded)   │  │ status: ● OK  │ │
│ │ replicas: 3             │  │ replicas: 2              │  │ replicas: 1   │ │
│ │ HPA: On  Flags: ODGM    │  │ HPA: Off Flags: ODGM,HPA │  │ HPA: Off      │ │
│ └──────────────────────────┘  └──────────────────────────┘  └───────────────┘ │
└───────────────────────────────────────────────────────────────────────────────┘
Card click → detail view with rollout indicators and ClassRuntime summary

#### Deployments – ClassRuntime Detail (High‑Level)

```
┌──────────── Deployment: my.class v1 ────────────────────────────────────────┐
│ Namespace: oaas‑system         ClassRuntime: my.class‑runtime               │
│ Status: ● OK  Conditions: [ Ready ✓ ] [ Progressing … ] [ Degraded × ]     │
│ Desired: gen=7  Observed: gen=7  Replicas: 3 (HPA: On)  Features: ODGM,HPA │
├──────────────── Referenced Functions ──────────────┬──────── Notes ────────┤
│ • init()   • handle_event(e)  • query(id)         │ links/docs/releases   │
├──────────────── Configuration (read‑only) ────────┴───────────────────────┤
│ Class: my.class   Partitioning: hash(pid)   ODGM: enabled                  │
│ Env flags: OPRC_CRM_FEATURES_HPA, OPRC_CRM_FEATURES_ODGM                   │
└────────────────────────────────────────────────────────────────────────────┘
```
```

### Topology – Zenoh Network Graph (Snapshot)

```
┌──────────────── Topology ────────────── [ Live ● ] [ Role: All ▾ ] [ Reset ] ┐
│ ┌──────────────────────────────────────────────────────────────────────────┐ │
│ │   o router‑1                                                              │
│ │    ╲                                                                     │
│ │     o peer‑a ─── o peer‑b                                                 │
│ │          ╲       ╲                                                        │
│ │           o peer‑c  ─ o peer‑d                                            │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ Legend: o peer (emerald)   o router (indigo)   link (slate)                 │
│ Toolbar: ◯ Zoom In  ⊖ Zoom Out  ⤢ Fit  ⌖ Center  ☰ List                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Settings – Appearance and Preferences

```
┌──────────────── Settings ───────────────┐
│ Theme: (●) System  ( ) Light  ( ) Dark │
│ Primary Color: [ Indigo ▾ ]            │
│ Date/Time Format: [ 2025‑11‑07 12:00 ] │
│ Units: [ ms ▾ ]                        │
│ [ Save Preferences ]                   │
└─────────────────────────────────────────┘
```

Notes:
- All screens use consistent spacing, clear focus rings, and accessible contrast.
- Dark mode mirrors the same layout with adjusted colors.

