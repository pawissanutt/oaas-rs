# OaaS-RS Console — Visual Design Document

> **Purpose**: This document describes the UX/UI design of the OaaS-RS management console, intended as a reference for reimplementing the GUI in any frontend framework.

---

## 1. Application Overview

The OaaS-RS Console is a **single-page application (SPA)** for managing an Object-as-a-Service platform. It provides operational visibility and control over:

- **Packages** — Bundles of classes and functions
- **Deployments** — Runtime instances of classes
- **Objects** — Stateful data entities
- **Functions** — Invocable operations
- **Environments** — Connected clusters
- **Topology** — Visual graph of system components

### Design Philosophy
- **Dashboard-centric**: Quick overview with actionable insights
- **Dark/Light theme support**: User preference stored locally
- **Responsive**: Mobile sidebar + desktop horizontal navbar
- **Consistent patterns**: Cards, modals, status badges, search bars

---

## 2. Global Layout

```
┌─────────────────────────────────────────────────────────────┐
│  NAVBAR (desktop: horizontal / mobile: hamburger + sidebar) │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                       MAIN CONTENT                          │
│                    (Route-based pages)                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 Navigation Bar

#### Desktop (≥768px)
- **Horizontal bar** at the top
- **Background**: Dark gray (#1F2937)
- **Logo/Title**: "OaaS Console" (left side, implicit)
- **Navigation Links** (left group):
  | Icon | Label | Route |
  |------|-------|-------|
  | 🏠 | Home | `/` |
  | 📦 | Objects | `/objects` |
  | 🚀 | Deployments | `/deployments` |
  | ⚡ | Functions | `/functions` |
  | 🌍 | Envs | `/environments` |
  | 🔗 | Topology | `/topology` |
  | 📋 | Packages | `/packages` |
- **Settings** (right): ⚙️ icon linking to `/settings`

#### Mobile (<768px)
- **Sticky header bar** with:
  - Hamburger menu (☰) on left
  - "OaaS Console" title centered
  - Settings icon on right
- **Slide-out sidebar** (left, 256px wide):
  - Dark background
  - Close button (✕) in header
  - Vertical navigation links with icons and labels
  - Settings link at bottom with separator

### 2.2 Theme System
- **Three modes**: Dark (default), Light, System
- Dark theme: Gray-900 backgrounds, gray-100 text
- Light theme: Gray-100 backgrounds, gray-900 text
- Theme toggle in Settings page with immediate preview

---

## 3. Pages & Components

### 3.1 Home (Dashboard)

**Purpose**: Quick overview of system state with actionable links

```
┌────────────────────────────────────────────────────────────┐
│  Dashboard                                                 │
│  OaaS-RS Management Console                                │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │ Packages │ │ Classes  │ │Functions │ │Deploys   │      │
│  │    📦    │ │    🏷️   │ │    ⚡    │ │    🚀    │      │
│  │    12    │ │    34    │ │    56    │ │    8     │      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘      │
│                                                            │
│  ┌─────────────────────────┐ ┌─────────────────────────┐  │
│  │   Deployment Status     │ │     System Health       │  │
│  │  ┌──────────────────┐   │ │  Overall: [Healthy]     │  │
│  │  │ ● Running    5   │   │ │                         │  │
│  │  │ ● Pending    2   │   │ │  Environments:          │  │
│  │  │ ● Error      1   │   │ │  ├─ cluster-1 ● Healthy │  │
│  │  └──────────────────┘   │ │  └─ cluster-2 ● Degraded│  │
│  │  [████████░░] 62%       │ │                         │  │
│  └─────────────────────────┘ └─────────────────────────┘  │
│                                                            │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  Quick Actions                                       │  │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │  │
│  │  │ 📦 Manage   │ │ 🚀 Deploy-  │ │ 🗃️ Browse   │    │  │
│  │  │ Packages    │ │ ments       │ │ Objects     │    │  │
│  │  └─────────────┘ └─────────────┘ └─────────────┘    │  │
│  └─────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

#### Components:

**Stat Cards** (grid of 4)
- Colored background (blue, purple, orange, green)
- Icon in circle (right side)
- Metric label (small text, top)
- Value (large bold number)

**Deployment Status Panel**
- Card with header "Deployment Status"
- Three status rows with colored indicators:
  - Green: Running count
  - Yellow: Pending count
  - Red: Error count
- Progress bar showing ratio

**System Health Panel**
- Overall status badge (green/yellow/red)
- Relative timestamp ("Updated 5m ago")
- List of environments with status dots

**Quick Actions**
- Grid of 4 action cards
- Each card: icon, label, description
- Links to relevant pages

---

### 3.2 Packages

**Purpose**: CRUD operations on packages (bundles of classes and functions)

```
┌────────────────────────────────────────────────────────────┐
│  📦 Packages                           [+ New Package]     │
├────────────────────────────────────────────────────────────┤
│  🔍 [Search packages...                           ]        │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ▸ 📦 my-package  v1.0.0       3 classes • 5 functions 🗑️ │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  (expanded content when clicked)                     │  │
│  │                                                       │  │
│  │  Classes:                                            │  │
│  │  ┌────────────────────────────────────────────────┐  │  │
│  │  │ 🏷️ EchoClass                                   │  │  │
│  │  │ Partitions: 8   Functions: echo, process       │  │  │
│  │  └────────────────────────────────────────────────┘  │  │
│  │                                                       │  │
│  │  Functions:                                          │  │
│  │  ┌────────────────────────────────────────────────┐  │  │
│  │  │ ⚡ echo [Custom]  "Echo function"              │  │  │
│  │  │ ⚡ process [Macro]                             │  │  │
│  │  └────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  ▸ 📦 another-pkg  —              1 classes • 2 functions  │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

#### List Item Pattern
- Collapsible (`<details>`) accordion
- Summary row shows: icon, name, version badge, class/function counts
- Delete button (🗑️) visible on hover
- Raw data button (📄) visible on hover

#### Create Modal (YAML Editor)
- Full-screen modal overlay
- Header: "Create Package"
- YAML editor with:
  - Line numbers (left gutter)
  - Syntax highlighting
  - Real-time validation status
- Checkbox: "Also deploy all classes"
- Buttons: Cancel (secondary), Apply (primary)

---

### 3.3 Deployments

**Purpose**: Manage class deployments with lifecycle operations

```
┌────────────────────────────────────────────────────────────┐
│  🚀 Deployments                       [+ New Deployment]   │
├────────────────────────────────────────────────────────────┤
│  🔍 [Search deployments...                        ]        │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ 🚀 my-package.EchoClass              [Running] 📄 🗑️ │  │
│  │ Package: my-package • Class: EchoClass                │  │
│  │                                                       │  │
│  │ ┌─────────────────────────────────────────────────┐   │  │
│  │ │ NFR Requirements                                │   │  │
│  │ │ Min Throughput: 1000 rps  Availability: 99.9%   │   │  │
│  │ │ CPU Target: 70%                                 │   │  │
│  │ └─────────────────────────────────────────────────┘   │  │
│  │                                                       │  │
│  │ Selected Environments:                                │  │
│  │ • cluster-1 (healthy) • cluster-2 (healthy)           │  │
│  │                                                       │  │
│  │ Last Reconciled: 2m ago                               │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

#### Status Badges
| Status | Color | Background |
|--------|-------|------------|
| Running | Green | green-100/green-900 |
| Deploying | Yellow | yellow-100/yellow-900 |
| Pending | Gray | gray-100/gray-700 |
| Down | Red | red-100/red-900 |
| Deleted | Red | red-100/red-900 |

#### Deployment Card
- Header: key + status badge
- Subtitle: Package and Class info
- NFR section (conditional): throughput, availability, CPU target
- Selected environments list
- Timestamps with relative time

---

### 3.4 Objects

**Purpose**: Browse and interact with stored objects

```
┌────────────────────────────────────────────────────────────┐
│  📦 Objects                                                │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Class: [▼ Select class...                         ] │  │
│  │ Partition: (•) All  ( ) Specific: [0]              │  │
│  │ Prefix: [                    ]  [Browse]  [+ New]  │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                            │
│  ┌──────────────────────┐  ┌───────────────────────────┐  │
│  │  Object List         │  │  Object Detail            │  │
│  │  ┌────────────────┐  │  │  ID: obj-001              │  │
│  │  │ obj-001 (P0)   │  │  │  Partition: 0             │  │
│  │  │ obj-002 (P1)   │  │  │                           │  │
│  │  │ obj-003 (P0)   │  │  │  Entries:                 │  │
│  │  │ ...            │  │  │  ├─ state: {...}          │  │
│  │  └────────────────┘  │  │  └─ counter: 42           │  │
│  │                      │  │                           │  │
│  │                      │  │  ┌─────────────────────┐  │  │
│  │                      │  │  │ Invoke Function     │  │  │
│  │                      │  │  │ [▼ echo         ]   │  │  │
│  │                      │  │  │ Payload (JSON):     │  │  │
│  │                      │  │  │ ┌─────────────────┐ │  │  │
│  │                      │  │  │ │ {...}           │ │  │  │
│  │                      │  │  │ └─────────────────┘ │  │  │
│  │                      │  │  │ [Invoke]            │  │  │
│  │                      │  │  └─────────────────────┘  │  │
│  │                      │  │                           │  │
│  │                      │  │  [Edit] [Delete]          │  │
│  └──────────────────────┘  └───────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

#### Three-Column Layout
1. **Filter Bar** (top): Class dropdown, partition selector, prefix input
2. **Object List** (left): Scrollable list with object IDs and partitions
3. **Object Detail** (right): Selected object's entries and actions

#### Object CRUD Modal
- Tabs: "Entries" | "Events"
- Entries tab: Key-value editor with add/remove rows
- Events tab: Function and data trigger configuration
- Triggers have nested target lists (on_complete, on_error, etc.)

---

### 3.5 Functions

**Purpose**: Browse all functions across packages

```
┌────────────────────────────────────────────────────────────┐
│  ⚡ Functions                                              │
├────────────────────────────────────────────────────────────┤
│  🔍 [Search...        ]  Type: [▼ All types]               │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ ⚡ echo                          [Custom]         📄 │  │
│  │ Package: my-package v1.0.0                            │  │
│  │ "Echo function that returns the input"                │  │
│  │                                                       │  │
│  │ Bound to: EchoClass.echo (stateless)                  │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ ⚡ process                       [Macro]          📄 │  │
│  │ Package: my-package v1.0.0                            │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

#### Function Type Badges
| Type | Color |
|------|-------|
| Builtin | Gray |
| Custom | Blue |
| Macro | Green |
| Logical | Purple |

#### Function Card
- Header: icon + function key + type badge
- Package source line
- Description (if available)
- List of class bindings with stateless indicator

---

### 3.6 Environments

**Purpose**: View connected CRM clusters and their health

```
┌────────────────────────────────────────────────────────────┐
│  🌍 Environments                            [↻ Refresh]    │
├────────────────────────────────────────────────────────────┤
│  🔍 [Search environments...                       ]        │
│  3 environment(s) • 2 healthy                              │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌────────────────────┐ ┌────────────────────┐            │
│  │ 🌍 cluster-1       │ │ 🌍 cluster-2       │            │
│  │ [✓ Healthy]    📄  │ │ [⚠ Degraded]   📄  │            │
│  │                    │ │                    │            │
│  │ CRM: v0.5.0        │ │ CRM: v0.5.0        │            │
│  │ Nodes: 3/3 ready   │ │ Nodes: 2/3 ready   │            │
│  │ Avail: 99.95%      │ │ Avail: 98.50%      │            │
│  │ Seen: 30s ago      │ │ Seen: 45s ago      │            │
│  └────────────────────┘ └────────────────────┘            │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

#### Environment Cards (Grid)
- 1 column (mobile), 2 columns (tablet), 3 columns (desktop)
- Header: name + status badge
- Health details: CRM version, node count, availability %, last seen
- Availability color coding: ≥99% green, ≥95% yellow, <95% red

---

### 3.7 Topology

**Purpose**: Interactive graph visualization of system components

```
┌────────────────────────────────────────────────────────────┐
│  Topology View                                             │
├────────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────────┐  │
│  │ 12 nodes and 8 connections • Last updated: 2m ago    │  │
│  │ [From Deployments] [From Zenoh]   [Hide/Show Panel]  │  │
│  │                                                       │  │
│  │ Legend:                                               │  │
│  │ ■ Package  ● Class  ■ Function  ◆ Environment        │  │
│  │ ◆ Router   ■ Gateway  ● ODGM                         │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  ┌──────────────────────────────────┐ ┌─────────────────┐  │
│  │                                  │ │ Node Details    │  │
│  │         [Interactive             │ │                 │  │
│  │          Graph Canvas            │ │ ID: gateway-1   │  │
│  │          (Cytoscape)]            │ │ Type: [Gateway] │  │
│  │                                  │ │ Status: Healthy │  │
│  │    (Package)──(Class)──(Func)    │ │                 │  │
│  │        │                         │ │ Metadata:       │  │
│  │    (Environment)──(Router)       │ │ • version: 1.0  │  │
│  │                                  │ │ • region: us-1  │  │
│  │                                  │ │                 │  │
│  └──────────────────────────────────┘ └─────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

#### Graph Canvas
- Fixed height (600px)
- Interactive pan/zoom
- Node shapes by type:
  - Package: rounded rectangle (indigo)
  - Class: circle (cyan)
  - Function: rectangle (amber)
  - Environment: diamond (pink)
  - Router: diamond (purple)
  - Gateway: rectangle (blue)
  - ODGM: circle (emerald)

#### Side Panel (collapsible)
- Appears on node click
- Shows: ID, type badge, status badge, metadata key-values
- List of deployed classes (if applicable)

#### Source Toggle
- Two buttons: "From Deployments" | "From Zenoh"
- Active button has primary color background

---

### 3.8 Settings

**Purpose**: User preferences and configuration

```
┌────────────────────────────────────────────────────────────┐
│  ⚙️ Settings                                               │
├────────────────────────────────────────────────────────────┤
│  [Settings saved!]  (notification, auto-dismiss 2s)        │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Appearance                                            │  │
│  │                                                       │  │
│  │ Theme               [▼ 🌙 Dark              ]         │  │
│  │                     Options: Dark, Light, System      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Data Display                                          │  │
│  │                                                       │  │
│  │ Default Page Size   [▼ 25 ]                           │  │
│  │                                                       │  │
│  │ [x] Auto-refresh data                                 │  │
│  │ Refresh Interval    [30] seconds                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Connection                                            │  │
│  │                                                       │  │
│  │ API Base URL        [same origin (relative)]          │  │
│  │                     (read-only)                       │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
│  [Reset to Defaults]                                       │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

#### Settings Sections
1. **Appearance**: Theme selector
2. **Data Display**: Page size, auto-refresh toggle, interval
3. **Connection**: Read-only API URL display

#### Persistence
- All settings saved to localStorage
- Theme applies immediately on change
- Save confirmation notification (green banner, 2s auto-dismiss)

---

## 4. Reusable Components

### 4.1 Status Badges

```
[Running]  ← green background, green text
[Pending]  ← gray background, gray text
[Error]    ← red background, red text
[Healthy]  ← green background with ✓ icon
[Degraded] ← yellow background with ⚠ icon
```

- Pill-shaped (rounded corners)
- Small text, semibold
- Padding: 8px horizontal, 4px vertical

### 4.2 Search Bar

- Full width on mobile, fixed width (384px) on larger screens
- Left icon: 🔍
- Placeholder text describing context
- Instant filtering (no submit button)

### 4.3 YAML Editor

- Two-column layout: line numbers | content
- Monospace font
- Syntax highlighting:
  - Keys: purple
  - Strings: green
  - Numbers: blue
  - Booleans: orange
  - Comments: gray italic
- Real-time validation indicator:
  - ✓ Valid (green)
  - ✗ YAML Error: message (red)
  - ✗ Schema Error: message (orange)

### 4.4 Raw Data Modal

- Full-screen overlay with centered card
- Header: title + close button
- Format toggle: JSON | YAML
- Syntax-highlighted content display
- Copy button with feedback ("Copied!")

### 4.5 Relative Time Display

- Shows "5s ago", "2m ago", "1h ago", etc.
- Auto-updates at appropriate intervals
- Tooltip shows full timestamp
- Format: "Updated 5m ago (2025-01-28 10:30:00)"

### 4.6 Delete Confirmation Modal

- Overlay with centered card
- Warning icon and message
- Two buttons: Cancel (secondary), Delete (red/destructive)
- Shows the name of item being deleted

### 4.7 Loading States

- Centered spinner animation
- Text below: "Loading [resource]..."
- Used during data fetches

### 4.8 Error Banners

- Red background (light: red-50, dark: red-900/30)
- Red border
- Error icon and message text

---

## 5. Color Palette

### Base Colors (Tailwind)
| Token | Light | Dark |
|-------|-------|------|
| Background | gray-100 | gray-900 |
| Card | white | gray-800 |
| Text Primary | gray-900 | gray-100 |
| Text Secondary | gray-600 | gray-400 |
| Border | gray-200 | gray-700 |

### Semantic Colors
| Purpose | Light | Dark |
|---------|-------|------|
| Primary | blue-600 | blue-500 |
| Success | green-600 | green-400 |
| Warning | yellow-600 | yellow-400 |
| Error | red-600 | red-400 |
| Info | blue-600 | blue-400 |

### Entity Colors (Topology)
| Entity | Color |
|--------|-------|
| Package | indigo-500 |
| Class | cyan-500 |
| Function | amber-500 |
| Environment | pink-500 |
| Router | purple-500 |
| Gateway | blue-500 |
| ODGM | emerald-500 |

---

## 6. Typography

| Element | Size | Weight | Font |
|---------|------|--------|------|
| Page Title | 24px (2xl) | Bold | System sans |
| Section Header | 18px (lg) | Semibold | System sans |
| Card Title | 20px (xl) | Semibold | System sans |
| Body | 14px (sm) | Normal | System sans |
| Code/Values | 14px (sm) | Normal | Monospace |
| Labels | 12px (xs) | Medium | System sans |
| Badges | 12px (xs) | Semibold | System sans |

---

## 7. Responsive Breakpoints

| Breakpoint | Width | Layout Changes |
|------------|-------|----------------|
| Mobile | <768px | Sidebar navigation, single column grids |
| Tablet | 768-1024px | Horizontal nav, 2-column grids |
| Desktop | >1024px | Full nav with icons, 3-4 column grids |

---

## 8. Interaction Patterns

### 8.1 Navigation
- Clicking nav item routes to page
- Active route not visually indicated (could be enhancement)
- Mobile sidebar closes on route change

### 8.2 Lists
- Click row to select/expand
- Hover reveals action buttons (delete, view raw)
- Search instantly filters displayed items

### 8.3 Forms
- Validation on input change
- Submit button disabled during loading
- Success redirects or shows notification
- Error displayed in modal or banner

### 8.4 Modals
- Click overlay to close
- Escape key closes (if implemented)
- Scroll for long content

### 8.5 Data Refresh
- Manual refresh buttons on some pages
- Auto-refresh optional (Settings)
- Relative times update automatically

---

## 9. Empty States

Each page handles empty data gracefully:

| Page | Empty Message |
|------|---------------|
| Packages | "No packages found" |
| Deployments | "No deployments found" |
| Objects | "No objects found" (after browsing) |
| Functions | "No functions found" |
| Environments | "No environments found" / "Environments are connected CRM clusters" |
| Topology | Shows empty graph or error |
| Dashboard | "No deployments yet" with link to create |

---

## 10. Implementation Notes

### State Management
- Page-level state for data and UI (loading, error, selected items)
- Global state for settings (theme, preferences)
- LocalStorage for persistence

### API Integration
- REST API calls to same origin (PM backend)
- Async data fetching with loading/error handling
- Parallel fetches where possible (dashboard)

### Graph Visualization
- Uses Cytoscape.js for topology graph
- Nodes and edges passed as JSON
- Selection callback updates side panel

### Accessibility Considerations
- Semantic HTML structure
- Keyboard navigation for forms
- Color contrast for text readability
- Title attributes on icon-only buttons

---

## 11. Page Route Summary

| Route | Page Component | Description |
|-------|----------------|-------------|
| `/` | Home | Dashboard with stats |
| `/objects` | Objects | Browse and invoke |
| `/deployments` | Deployments | Manage deployments |
| `/functions` | Functions | Browse functions |
| `/environments` | Environments | View clusters |
| `/topology` | Topology | Graph visualization |
| `/packages` | Packages | Manage packages |
| `/settings` | Settings | User preferences |

---

## 12. Backend API Reference

The frontend communicates with the **Package Manager (PM)** REST API. All endpoints use a common base URL (same origin in production, configurable for development).

### 12.1 API Base URL

```
Base URL: {origin}/api/v1/...       # PM direct endpoints
Gateway:  {origin}/api/gateway/...  # PM proxies to Gateway for data-plane ops
```

### 12.2 Packages API

| Method | Endpoint | Description | Request | Response |
|--------|----------|-------------|---------|----------|
| GET | `/api/v1/packages` | List all packages | — | `OPackage[]` |
| POST | `/api/v1/packages` | Create/update package | `OPackage` (JSON) | `{ name, message }` |
| DELETE | `/api/v1/packages/{name}` | Delete a package | — | `204 No Content` |

**OPackage Model** (simplified):
```yaml
name: string
version: string (optional)
classes:
  - key: string
    partition_count: number
    function_bindings: [{ name, function_key, stateless }]
functions:
  - key: string
    function_type: "Builtin" | "Custom" | "Macro" | "Logical"
    description: string (optional)
```

### 12.3 Deployments API

| Method | Endpoint | Description | Request | Response |
|--------|----------|-------------|---------|----------|
| GET | `/api/v1/deployments` | List all deployments | — | `OClassDeployment[]` |
| POST | `/api/v1/deployments` | Create/update deployment | `OClassDeployment` (JSON) | `{ message, deployment_key }` |
| DELETE | `/api/v1/deployments/{key}` | Delete deployment | — | `204 No Content` |

**OClassDeployment Model** (simplified):
```yaml
key: string                    # e.g., "package.ClassName"
package_name: string
class_key: string
nfr_requirements:
  min_throughput_rps: number (optional)
  availability: number (optional)
  cpu_utilization_target: number (optional)
status:                        # Read-only, set by backend
  selected_envs: string[]
  last_error: string (optional)
  last_reconciled_at: datetime
```

### 12.4 Environments API

| Method | Endpoint | Description | Response |
|--------|----------|-------------|----------|
| GET | `/api/v1/envs` | List all environments | `ClusterInfo[]` |
| GET | `/api/v1/envs/health` | Get health for all clusters | `ClusterHealth[]` |
| GET | `/api/v1/envs/{name}/health` | Get health for specific cluster | `ClusterHealth` |

**ClusterInfo Model**:
```yaml
name: string
health:
  status: "Healthy" | "Degraded" | "Unhealthy"
  crm_version: string (optional)
  node_count: number (optional)
  ready_nodes: number (optional)
  availability: number (optional)  # 0.0 - 1.0
  last_seen: datetime
```

### 12.5 Class Runtimes API

| Method | Endpoint | Description | Response |
|--------|----------|-------------|----------|
| GET | `/api/v1/class-runtimes` | List deployed class runtimes | `ClassRuntime[]` |

**ClassRuntime Model**:
```yaml
class_key: string          # e.g., "package.ClassName"
package_name: string
partition_count: number
```

### 12.6 Topology API

| Method | Endpoint | Description | Response |
|--------|----------|-------------|----------|
| GET | `/api/v1/topology?source={source}` | Get topology graph | `TopologySnapshot` |

**Query Parameters**:
- `source`: `"deployments"` (from deployment data) or `"zenoh"` (from live Zenoh network)

**TopologySnapshot Model**:
```yaml
nodes:
  - id: string
    node_type: "package" | "class" | "function" | "environment" | "router" | "gateway" | "odgm"
    status: "healthy" | "degraded" | "down"
    metadata: { key: value, ... }
    deployed_classes: string[] (optional)
edges:
  - source: string  # node id
    target: string  # node id
    edge_type: string
timestamp: { seconds, nanos }  # protobuf Timestamp
```

### 12.7 Gateway Proxy (Data Plane Operations)

The PM proxies requests to the Gateway for object and invocation operations:

#### Object Operations

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/gateway/api/class/{class_key}/{partition_id}/objects` | List objects |
| GET | `/api/gateway/api/class/{class_key}/{partition_id}/objects/{object_id}` | Get object |
| PUT | `/api/gateway/api/class/{class_key}/{partition_id}/objects/{object_id}` | Create/update object |
| DELETE | `/api/gateway/api/class/{class_key}/{partition_id}/objects/{object_id}` | Delete object |

**List Objects Query Parameters**:
- `prefix`: Filter by object ID prefix
- `limit`: Max number of results
- `cursor`: Pagination cursor

**Object Response (ObjData)**:
```yaml
object_id: string
meta:
  version: number
  create_time: timestamp
  update_time: timestamp
entries:
  key: { data: base64, is_json: boolean }
events:
  func_triggers: [{ fn_id, on_complete: [targets], on_error: [targets] }]
  data_triggers: [{ entry_key, on_create: [targets], on_update: [targets], on_delete: [targets] }]
```

#### Function Invocation

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/gateway/api/class/{class_key}/{partition_id}/invokes/{function_key}` | Stateless invoke |
| POST | `/api/gateway/api/class/{class_key}/{partition_id}/objects/{object_id}/invokes/{function_key}` | Stateful invoke |

**Request Body**: JSON payload (function input)
**Response**: Raw bytes (function output)

### 12.8 Error Handling

All API endpoints return standard HTTP status codes:

| Status | Meaning |
|--------|---------|
| 200 | Success |
| 201 | Created |
| 204 | No Content (successful delete) |
| 400 | Bad Request (invalid input) |
| 404 | Not Found |
| 500 | Internal Server Error |

**Error Response Format**:
```json
{
  "error": "Error message",
  "details": "Optional additional details"
}
```

### 12.9 Frontend API Usage Pattern

```
┌─────────────────┐     REST/JSON      ┌──────────────────┐
│                 │ ─────────────────► │                  │
│    Frontend     │                    │  Package Manager │
│   (Browser)     │ ◄───────────────── │      (PM)        │
│                 │                    │                  │
└─────────────────┘                    └────────┬─────────┘
                                                │
                                    /api/gateway/* proxy
                                                │
                                                ▼
                                       ┌──────────────────┐
                                       │     Gateway      │
                                       │  (Data Plane)    │
                                       └──────────────────┘
```

**Key Points**:
1. All requests go to PM (same origin)
2. PM handles control-plane operations directly (packages, deployments, envs)
3. PM proxies data-plane operations to Gateway (`/api/gateway/*`)
4. Authentication: None currently (add auth headers if needed)
5. Content-Type: `application/json` for all requests/responses

### 12.10 Page-to-API Mapping

| Page | APIs Used |
|------|-----------|
| Home (Dashboard) | `GET /packages`, `GET /deployments`, `GET /envs/health` |
| Packages | `GET /packages`, `POST /packages`, `DELETE /packages/{name}` |
| Deployments | `GET /deployments`, `POST /deployments`, `DELETE /deployments/{key}` |
| Functions | `GET /packages` (extract functions from packages) |
| Environments | `GET /envs` |
| Topology | `GET /topology?source=...` |
| Objects | `GET /class-runtimes`, `GET/PUT/DELETE .../objects/...`, `POST .../invokes/...` |
| Settings | None (localStorage only) |

---

*Document generated for OaaS-RS Console reimplementation reference.*
