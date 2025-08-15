# OaaS Class Runtime Manager (CRM)

Kubernetes-native controller that manages the deployment lifecycle of OaaS classes/functions via CRDs and a gRPC API. It follows the single code path design with runtime profiles (dev/edge/full) and feature flags.

Important modeling note
- A DeploymentRecord CRD represents one Class deployment.
- A Class may consist of multiple function runtimes and one ODGM instance.
- ODGM runs as its own Kubernetes Deployment/Service, not as a sidecar in the function pod.

## PM ↔ CRM interaction (contract and flows)

Defines how the Package Manager (PM) talks to CRM so both teams can implement independently. PM is REST outward but uses gRPC to CRM with shared protobufs from `commons/oprc-grpc`. One CRM manages exactly one Kubernetes cluster.

- Service: `deployment.DeploymentService`
  - `Deploy(DeployRequest) -> DeployResponse` — upsert a `DeploymentRecord` CRD (idempotent by `deployment_id`)
  - `GetDeploymentStatus(GetDeploymentStatusRequest) -> GetDeploymentStatusResponse` — read CRD status and map to response
  - `DeleteDeployment(DeleteDeploymentRequest) -> DeleteDeploymentResponse` — mark for deletion and finalize children
- Service: `grpc.health.v1.Health` — health/readiness

Conventions:
- Idempotency key: `deployment_id` (stable across retries); same id must converge to the same desired state
- Correlation: pass `x-correlation-id` metadata; CRM mirrors it to CRD annotations and logs; echo in responses
- Namespacing: request includes `namespace` (or CRM default); all CRDs and owned resources are namespaced
- Deadlines: PM sets RPC timeouts (e.g., 5s status, 15s deploy); CRM should return `DEADLINE_EXCEEDED` rather than hang

Mapping: DeployRequest → DeploymentRecord CRD
- metadata.name: derived from `deployment_id` (DNS-1123 safe), label `oaas.io/deployment-id`
- metadata.annotations: `x-correlation-id`, profile, enforcement mode, template hint
- spec fields: deployment_unit (class/functions), nfr_requirements, target_environment, template_hint, addons (simple list), odgm_config (collections), function_containers
- Resulting resources: one or more function Deployments/Services (or Knative Services) plus a separate ODGM Deployment/Service when enabled. All rendered resources include label `oaas.io/owner=<class-name>` to enable label-based lifecycle ops.
- status fields (controller-owned): Conditions (Available/Progressing/Degraded), phase, message, observedGeneration, resource_refs, nfr_recommendations, odgm status

Flow summaries
1) Deploy
  - PM → CRM Deploy: upsert CRD, ensure finalizer, return `accepted=true` and a status snapshot
  - Controller reconciles: SSA apply resources for the Class: function Deployments/Services (or Knative) and an ODGM Deployment/Service (separate workload), set ownerRefs and a stable owner label, update Conditions/Events
2) Status
  - PM → CRM GetDeploymentStatus: CRM reads CRD; maps Conditions/phase and returns resource refs and recommendations
3) Delete
  - PM → CRM DeleteDeployment: CRM sets deletion (or ensures finalizer), controller deletes children selected by label `oaas.io/owner=<class-name>`, removes finalizer; subsequent status → NOT_FOUND

Error codes and retry
- INVALID_ARGUMENT: request schema/validation errors
- FAILED_PRECONDITION: missing/unsupported template, ODGM required but disabled
- NOT_FOUND: status/delete for unknown `deployment_id`
- ALREADY_EXISTS: name collision with different id hash
- UNAVAILABLE: k8s API not reachable or leader not ready — PM retries with backoff
- DEADLINE_EXCEEDED: deadline hit — PM retries with jitter

Multi-cluster coordination
- PM manages a map of clusters → CRM endpoints/credentials and chooses the target cluster; CRM is unaware of other clusters.
- Suggested PM envs: `OPRC_PM_CRM_DEFAULT_CLUSTER`, `OPRC_PM_CRM_<NAME>_URL`, `OPRC_PM_CRM_<NAME>_TOKEN`.

Observability
- Tracing: include `deployment_id` and `x-correlation-id` in all logs and Events. The controller and analyzer are instrumented with tracing spans; set `RUST_LOG` (e.g., `debug`, `info`, `warn`) to control verbosity.
- Metrics: controller reconcile durations, queue depth, errors; optional Prometheus URL for NFR observe/enforce. Analyzer runs in a background loop and patches status with recommendations.
- Events: the controller emits Kubernetes Events for apply/enforce actions (e.g., reason `Applied`, `NFRApplied`).

## Quick start
- Build: `cargo build -p oprc-crm`
- Run locally: `RUST_LOG=debug HTTP_PORT=8088 cargo run -p oprc-crm`
- Generate CRD YAML (PowerShell):

```powershell
cargo run -p oprc-crm --bin crdgen | Out-File -FilePath k8s/crds/deploymentrecords.gen.yaml -Encoding utf8
```

- Install CRD and RBAC (example):

```powershell
kubectl apply -f k8s/crds/deploymentrecords.gen.yaml
kubectl apply -f k8s/rbac/crm-rbac.yaml
```

- ODGM enablement requires both:
  - Env: `OPRC_CRM_FEATURES_ODGM=true` (profile default true only in full/prod)
  - CRD: `spec.addons` includes `"odgm"`

### Enable enforcement (quickstart)

1) Set feature flags and knobs (example dev values):
  - `OPRC_CRM_FEATURES_NFR_ENFORCEMENT=true`
  - `OPRC_CRM_FEATURES_HPA=true` (optional; enables HPA minReplicas patch when HPA exists)
  - `OPRC_CRM_ENFORCEMENT_STABILITY_SECS=180`
  - `OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS=120`
2) Apply RBAC and CRD:
  - `kubectl apply -f k8s/crds/deploymentrecords.gen.yaml`
  - `kubectl apply -f k8s/rbac/crm-rbac.yaml`
3) Run CRM with logs:
  - `RUST_LOG=debug HTTP_PORT=8088 cargo run -p oprc-crm`
4) Create a DeploymentRecord with enforcement mode:
  - `spec.nfr.enforcement.mode: enforce`
  - Optional: `spec.nfr.enforcement.dimensions: [replicas]`
5) Observe:
  - `kubectl get events --field-selector involvedObject.kind=DeploymentRecord`
  - `kubectl get deployment <name> -o yaml | yq .spec.replicas`
  - If HPA exists: `kubectl get hpa <name> -o yaml | yq .spec.minReplicas`

Sample DeploymentRecord:

```yaml
apiVersion: oaas.io/v1alpha1
kind: DeploymentRecord
metadata:
  name: hello-class
spec:
  selected_template: dev
  addons: ["odgm"]
  function:
    image: ghcr.io/pawissanutt/oprc-function:latest
    port: 8080
  odgm_config:
    collections: ["orders", "users"]
```

## Current status (scaffold)
- kube-rs controller loop for `DeploymentRecord` with basic SSA apply, owner refs, and finalizer handling
- Env-based config (`OPRC_CRM_*`) and tracing setup
- Single Axum server on `HTTP_PORT` serving both HTTP and embedded gRPC routes (Health, Reflection, DeploymentService)
- Default Kubernetes namespace via `OPRC_CRM_K8S_NAMESPACE`; correlation id is propagated to CRD annotations and echoed in responses
- Conditions use enums (Available/Progressing/Degraded/Unknown) instead of string matching; status summary derived from conditions
- gRPC deadlines respected via `grpc-timeout` header; K8s calls are wrapped with per-RPC timeouts
- Idempotency enforced: name collisions with different `oaas.io/deployment-id` return ALREADY_EXISTS
- Optional HTTP `/healthz` stub remains for local convenience

## Scope and non-goals
- Scope: Kubernetes-only, single-cluster controller; HTTP API for PM ↔ CRM; CRDs as source of truth; optional NFR observe/enforce; optional ODGM (as a separate Deployment).
- Non-goals: Multi-cluster orchestration, alternate backends (Docker Compose), non-Kubernetes runtimes.

## Configuration (env)
- `OPRC_CRM_ANALYZER_INTERVAL_SECS` (60) – analyzer loop interval used to refresh NFR recommendations

- `OPRC_CRM_PROM_URL` — Prometheus base URL (optional in dev/edge)
- `OPRC_CRM_SECURITY_MTLS` — enable gRPC mTLS (default false in dev, true in full)
- `RUST_LOG` — log level (`debug` in dev by default)
 - Interval is controlled by `OPRC_CRM_ANALYZER_INTERVAL_SECS`.
Planned: TLS/mTLS flags hardening and related security knobs.

Enforcement knobs (M5)
- `OPRC_CRM_ENFORCEMENT_STABILITY_SECS` (default 180) — required time that recommendations must remain stable before applying.
- `OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS` (default 120) — minimum time between successive enforcement actions for the same deployment (prevents churn).
- `OPRC_CRM_REQ_CPU_PER_POD_M` (default 500) — assumed CPU request (mCPU) per function pod for capacity heuristics when deriving replicas.

Feature flags
- `OPRC_CRM_FEATURES_NFR_ENFORCEMENT` (bool) — enable enforcement capability (observe-only when false).
- `OPRC_CRM_FEATURES_HPA` (bool) — allow HPA-aware enforcement (patch `minReplicas` when present).
- `OPRC_CRM_FEATURES_KNATIVE` (bool) — enable Knative template path.
- `OPRC_CRM_FEATURES_PROMETHEUS` (bool) — enable metrics-backed analyzer.

## gRPC API (initial)
Using `commons/oprc-grpc` generated protobufs and helpers.

Services:
- `deployment.DeploymentService`
  - `Deploy(DeployRequest) → DeployResponse` – create/update `DeploymentRecord` (idempotent)
  - `GetDeploymentStatus(GetDeploymentStatusRequest) → GetDeploymentStatusResponse`
  - `DeleteDeployment(DeleteDeploymentRequest) → DeleteDeploymentResponse`
- `health.Health` – gRPC health check
- Reflection – enabled for local dev

Conventions:
- Metadata propagation: `x-correlation-id` → CRD annotations and logs
- Error mapping: domain errors → canonical gRPC codes (NOT_FOUND, ALREADY_EXISTS, INVALID_ARGUMENT, INTERNAL)
- Deadlines: respect `grpc-timeout` and return `DEADLINE_EXCEEDED` promptly

## CRDs (v1alpha1)
- DeploymentRecord (namespaced)
  - spec (current implementation):
    - selected_template: optional explicit template selector (e.g., "dev", "edge", "cloud")
    - addons: optional string list for requested addons (simple), e.g., ["odgm"]
    - function: optional function runtime container hints
      - image: optional OCI image
      - port: optional container port (default 8080)
    - odgm_config: optional ODGM configuration
      - collections: optional list of logical collections ["orders", "users", ...]
  - status: Conditions (Available/Progressing/Degraded), phase, message, observedGeneration, last_updated
- DeploymentTemplate (namespaced or cluster-scoped later)
  - spec: template_type, nfr_capabilities, resource_requirements, manifests, selection_criteria

CRD YAMLs will be provided under `k8s/crds/` with status subresource enabled.

Manifests:
- CRD: `k8s/crds/deploymentrecords.yaml` (curated) and `k8s/crds/deploymentrecords.gen.yaml` (auto-generated)
- RBAC: `k8s/rbac/crm-rbac.yaml`

CRD generation:
- Generate from Rust types and write to file (PowerShell):

```powershell
cargo run -p oprc-crm --bin crdgen | Out-File -FilePath k8s/crds/deploymentrecords.gen.yaml -Encoding utf8
```

## Controller behavior
- Level-driven, idempotent reconcile
- Server-side apply (SSA) for manifests, owner refs, finalizer
- TemplateManager registry renders resources based on template_hint/NFR/profile
- Function pods receive addon-related configuration via env/ConfigMap (e.g., ODGM collections)
- ODGM is rendered as a separate Deployment/Service when enabled (not a sidecar)
- Emit Kubernetes Events for major transitions (Apply/Enforce)
- Local in-memory cache of `DeploymentRecord` objects drives analyzer/enforcer loops to reduce API list calls
- Conservative requeues/backoff; optional leader election

## Template system
 TemplateManager as a registry of multiple templates (Dev, Edge, Full; extensible)
 Selection order:
  1) spec.selected_template (explicit override)
  2) Score-based selection where templates weight the current environment and NFRs
    - Environment-aware scoring: when OPRC_CRM_PROFILE=dev, DevTemplate adds a large bonus; when edge, EdgeTemplate adds a large bonus. This makes dev/edge win by default while still allowing NFRs to influence ties/others.
    - NFR signals considered: min_throughput_rps, max_latency_ms, availability_pct, consistency
  3) Defaults to dev if nothing matches
 K8s Deployment/Service generation (first target) and optional Knative (behind feature)
 ODGM Deployment wiring behind a flag (ODGM is not a sidecar). Enablement requires both:
  1) `OPRC_CRM_FEATURES_ODGM=true` (profile default: true only in full/prod), and
  2) `spec.addons` contains "odgm" for the deployment.

Current behavior:
 Minimal selection: environment profile decides default replica counts; `selected_template` can override; NFR is used only as a last-resort tiebreaker.
  - dev: function=1, odgm=1 (ODGM image default: ghcr.io/pawissanutt/oprc-odgm:dev)
  - edge: function=2, odgm=1 (ODGM image default: ghcr.io/pawissanutt/oprc-odgm:edge)
  - full/prod: function=3, odgm=1 (ODGM image default: ghcr.io/pawissanutt/oprc-odgm:latest)
  Notes: The ODGM image can be overridden programmatically via the render context; future work may surface this via env/config.

  Implementation notes:
  - Template trait includes: name/aliases, render(ctx), and score(env, nfr)
  - EnvironmentContext currently provides the active profile for scoring
  - Files:
    - src/templates/manager.rs — registry, selection, shared render_with helper
    - src/templates/dev.rs — DevTemplate
    - src/templates/edge.rs — EdgeTemplate
    - src/templates/full.rs — FullTemplate
   - Implemented: src/templates/knative.rs — KnativeTemplate (separate from k8s Deployment)
     - Renders a Knative Service in place of Deployment/Service when Knative is enabled and CRDs are present
     - Adds basic Prometheus scrape annotations; extend later for request/latency/CPU/memory wiring
     - Selection: environment + NFR; considered only when Knative capability is detected at runtime
     - ODGM remains a separate Deployment/Service; function container gets ODGM_* envs

## NFR analysis & enforcement
- Modes: off (default in dev), basic (edge), full (prod)
- Observe-only first (populate recommendations in status)
- Prometheus-based monitoring (optional in dev/edge)
- Safeguards: cooldown, hysteresis, bounded delta, max replicas
 - Per-deployment controls: tuning can be enabled/disabled per DeploymentRecord, and users may override container requests/limits explicitly; overrides take precedence over recommendations/enforcement. See docs/NFR_ENFORCEMENT_DESIGN.md.

### M5 enforcement (replicas)

- Scope: basic enforcement of minimum replicas for the function runtime.
- Safeguards: requires a stability window and enforces a cooldown between applies. Bounded changes and hard caps apply.
- Apply paths:
  - Knative enabled: set `minScale` via annotation on the Knative Service.
  - Non‑Knative with HPA feature: if an HPA exists, patch `spec.minReplicas`; otherwise fall back to patching `Deployment.spec.replicas`.
- Transparency:
  - Events: publishes `NFRApplied` with a note describing the change (e.g., `replicas_min=3`).
  - Status audit: records the last applied recommendations and timestamp via `status.last_applied_recommendations` and `status.last_applied_at`.
- Opt-in: requires `OPRC_CRM_FEATURES_NFR_ENFORCEMENT=true` and per‑resource `spec.nfr.enforcement.mode = enforce`. Dimensions can be restricted via `spec.nfr.enforcement.dimensions`.

Example status audit and Event

```yaml
status:
  last_applied_recommendations:
    - component: "function"
      dimension: "replicas"
      target: 3
      basis: "enforcer"
      confidence: 1.0
  last_applied_at: "2025-08-14T12:34:56Z"
```

Event (abridged):

```
Type     Reason      Object                   Message
Normal   NFRApplied  DeploymentRecord/foo     Applied replicas_min=3 for foo
```

### Metrics provider abstraction (M4 design)

Goals
- Assume Prometheus Operator is installed and supported; other backends are out of scope for M4/M5.
- Provide a small, testable interface to fetch normalized signals (RPS, latency, CPU, memory) and to ensure scrape targets exist for managed workloads.

Provider interface (internal)
- ensure_targets(ns, targets) -> Result<()>
  - Creates/patches ServiceMonitor and/or PodMonitor for the function runtime and ODGM when using an operator-backed provider. No-op for direct/OTel providers.
- query_range(Query { expr, start, end, step }) -> SeriesSet
- query_instant(Expr) -> SampleSet
- health_check() -> ProviderHealth

Providers
- PrometheusOperatorProvider (only provider for now)
  - Discovers and manages ServiceMonitor/PodMonitor CRDs (group monitoring.coreos.com/v1) to scrape:
    - Function runtime: targets the Service or Pods labeled with `app=<class-name>` and `oaas.io/owner=<class-name>`; endpoint path `/metrics` on port `http` (templates set this).
    - ODGM: targets ODGM Service with `/metrics` (ODGM deployment exposes metrics port by convention).
  - Executes PromQL via HTTP against a configured Prometheus URL.
  - Knative behavior: when Knative is enabled and `OPRC_CRM_PROM_SCRAPE_KIND=auto`, the provider delegates monitoring to Knative-level configuration and does not create `ServiceMonitor`/`PodMonitor` resources. If you explicitly set `service` or `pod`, that choice is honored.
- OTelMetricsProvider [planned]
  - Queries an OpenTelemetry Collector or pulls metrics via OTLP/PromQL bridge; no CRDs management.

Analyzer execution model
- The Analyzer is initialized once at controller startup, caching Prometheus base URL and Knative-enabled flag from env.
- It runs in its own background loop (not in reconcile) and periodically:
  - Lists `DeploymentRecord` objects
  - Queries Prometheus for signals
  - Patches `status.nfr_recommendations` and sets the `NfrObserved` condition
- This decouples metrics latency from reconcile and avoids repeated env reads.

Signals and default queries (subject to tuning)
- rps (Knative only): `sum(rate(activator_request_count{namespace_name="<ns>", configuration_name="<name>"}[1m]))` — RPS is emitted only when Knative is enabled; for non‑Knative, RPS-based recommendations are skipped by default.
- p99_latency_ms (Knative): `1000 * max(histogram_quantile(0.99, sum(rate(activator_request_latencies_bucket{namespace_name="<ns>", configuration_name="<name>"}[1m])) by (revision_name, le)))` — aggregate per revision, then take max.
- p99_latency_ms (non‑Knative fallback): `1000 * histogram_quantile(0.99, sum(rate(http_server_requests_seconds_bucket{oaas_owner="<name>", namespace="<ns>"}[5m])) by (le))`
- cpu_mcores: `1000 * sum(rate(container_cpu_usage_seconds_total{pod=~"<prefix>.*",container!="",namespace="<ns>"}[5m]))`
- memory_working_set_bytes: `sum(container_memory_working_set_bytes{pod=~"<prefix>.*",container!="",namespace="<ns>"})`
Notes
- Templates add consistent labels/selectors (app and oaas.io/owner) to enable both ServiceMonitor and PodMonitor discovery.
- Knative: when using a Knative Service and `OPRC_CRM_PROM_SCRAPE_KIND=auto`, the provider skips creating monitors and relies on Knative’s own metrics pipeline. Set `service` or `pod` explicitly to force creation.

Startup detection and auto-disable
- On CRM startup, detect Prometheus Operator CRDs (monitoring.coreos.com/v1 ServiceMonitor/PodMonitor):
  - If present and OPRC_CRM_FEATURES_PROMETHEUS=true, enable the provider.
  - If missing, log a warning, set a controller-level Condition (PrometheusDisabled) with reason MissingCRDs, and auto-disable metrics features regardless of env flag.
  - Knative path: supported; when enabled and `auto`, provider skips creating monitors; explicit `service`/`pod` overrides are honored.

Configuration (env)
- OPRC_CRM_FEATURES_PROMETHEUS (bool, default false): master switch to enable metrics-backed analysis.
- OPRC_CRM_PROM_PROVIDER (string): prometheus-operator (only). Any other value will be ignored with a warning.
- OPRC_CRM_PROM_URL (string): base URL for Prometheus HTTP API (e.g., http://prometheus-k8s.monitoring.svc:9090). Required for Prometheus providers.
- OPRC_CRM_PROM_MATCH_LABELS (string): comma-separated key=value labels to add on ServiceMonitor/PodMonitor so Prometheus Operator selects them (e.g., "release=prometheus"). Optional.
- OPRC_CRM_PROM_SCRAPE_KIND (string): service | pod | auto (default auto). Controls whether to manage ServiceMonitor, PodMonitor, or pick based on runtime (Knative → pod).
- OPRC_CRM_PROM_QUERY_TIMEOUT_SECS (u64, default 5)
- OPRC_CRM_PROM_RANGE (string, default "10m") and OPRC_CRM_PROM_STEP (string, default "30s") for observe-only computations.

RBAC
- When using the Prometheus Operator provider, CRM needs permissions on monitoring.coreos.com:
  - servicemonitors, podmonitors: get, list, watch, create, update, patch, delete
- For enforcement and transparency, CRM also needs:
  - events.events.k8s.io and core events: create, patch
  - autoscaling HorizontalPodAutoscaler: get, list, watch, patch (if `OPRC_CRM_FEATURES_HPA=true`)
- Update `k8s/rbac/crm-rbac.yaml` accordingly in the same release.

Failure modes
- Provider unavailable or query errors: analyzer returns no data; controller sets a Condition note but does not degrade ownership; recommendations remain empty.
- Missing CRDs (ServiceMonitor/PodMonitor): metrics features are auto-disabled; analyzer becomes a no-op and recommendations remain empty; controller Condition updated.

Troubleshooting
- No enforcement actions:
  - Check stability/cooldown windows. Increase logs (`RUST_LOG=debug`) and look for "waiting for stability" or "in cooldown".
  - Ensure `spec.nfr.enforcement.mode=enforce` and replicas dimension is selected.
- Events missing:
  - Verify RBAC includes core/events.k8s.io events (create/patch/update).
  - Confirm controller service account/namespace matches the RBAC RoleBinding.
- HPA not patched:
  - Ensure `OPRC_CRM_FEATURES_HPA=true` and an HPA exists for the workload name.
  - Without HPA, CRM falls back to patching `Deployment.spec.replicas`.
- Prometheus disabled:
  - If Prometheus Operator CRDs are missing, analyzer auto-disables; look for `PrometheusDisabled` in status conditions.

Recommendations output and approval (observe → enforce)
- Status population (observe-only/M4):
  - status.nfr_recommendations: array scoped per component (function/odgm), with fields like type (replicas/memory/cpu), target, basis (window/metric), and confidence.
  - status.resource_refs: includes ServiceMonitor/PodMonitor names for traceability when enabled.
  - Conditions: NFRObserved=True when queries succeed; PrometheusDisabled=True when auto-disabled.
- Approval and enforcement (M5):
  - Default workflow: recommendations are written to status only; enforcement gate requires user opt-in.
  - Opt-in mechanisms:
    - Global env: OPRC_CRM_FEATURES_NFR_ENFORCEMENT=true (enables controller capability)
    - Per-DeploymentRecord: spec.nfr.enforcement.mode = off|observe|enforce (default observe). enforce applies recommendations within safeguards.
    - Per-dimension toggles: spec.nfr.enforcement.dimensions: [replicas, memory, cpu] to pick which to enforce.
  - Change transparency:
  - CRM writes an Event when applying any recommendation; `status.last_applied_recommendations` records the applied set with `status.last_applied_at` timestamp.
    - Safeguards (cooldown, bounded delta, max replicas/requests) always apply; explicit user requests/limits override enforcement.

Testing
- Provider is injected behind a trait; unit tests use a stub provider that returns canned time series.
- Query builder functions (labels, windows) are tested with edge cases (empty/zero traffic, bursty traffic).
- Knative path: PodMonitor selection tested via label selectors only (no cluster required).
 - Enable logs in tests via `test-log` (dev-dependency) with the `trace` feature; annotate tests with `#[test_log::test]` or `#[test_log::test(tokio::test)]` to capture tracing output.

## Polishing plan (MVP tidy-ups)
- [x] RBAC
  - [x] Add Events (core and events.k8s.io) create/patch/update and autoscaling/HPA get/list/watch/patch/update to `k8s/rbac/crm-rbac.yaml`.
  - [x] Regenerate and validate manifests for the target namespace(s).
- [x] Integration validation
  - [x] Run a quick kind/k3d smoke: verify Event emission (Applied/NFRApplied), HPA minReplicas patch path, and fallback to Deployment.spec.replicas when HPA is absent.
  - [x] Add targeted integration tests (HPA-present vs HPA-absent and Event publication; live cluster).
- [x] Docs
  - [x] Add a short "Enable enforcement" quickstart (env flags + per-CRD mode/dimensions example) and a Troubleshooting section (RBAC, stability/cooldown, Prometheus disabled).
  - [x] Include an example snippet of `status.last_applied_*` and a sample Event for traceability.
- [x] Observability
  - [x] Confirm PrometheusDisabled condition behavior and document expected logs/messages when metrics are auto-disabled.
- [ ] Optional
  - [ ] Consider creating HPA when missing (behind a flag), otherwise keep current fallback behavior explicit in docs.

## Refactoring opportunities
- [x] Controller structure
  - [x] Split `controller/mod.rs` into smaller modules (reconcile.rs, analyzer.rs, enforcer.rs, events.rs, status.rs, hpa_helper.rs, cache.rs, types.rs) to reduce file size and isolate concerns.
- [x] Enforcement helpers
  - [x] Extract HPA detection/patching into a dedicated helper with typed autoscaling v2 structs and clear error mapping/backoff.
  - [x] Factor Event emission into a small utility (constants for reasons: Applied, NFRApplied) to avoid repetition.
- [x] Status updates
  - [x] Prefer building typed `DeploymentRecordStatus` updates then serializing for `Patch::Merge` to keep JSON schema consistent; minimize ad-hoc json! fragments.
- [x] Cache boundary
  - [x] Wrap the in-memory DR cache (Arc<RwLock<_>>) behind a tiny repository with read/update helpers to centralize typed cache mutations and enable unit testing.
- [x] Config/types
  - [x] Replace stringly `mode` with a small enum for off|observe|enforce in internal logic; keep CRD as string but map early (internal only).
- [~] Resilience
  - [x] Add jittered retries for HPA patch path; classify NotFound/Forbidden as non-retryable with structured logs.
  - [ ] Add retries for Deployment/Knative apply paths.

## ODGM integration
- Config generation (collections; replication is network-abstracted)
- Separate ODGM Deployment + Service for the Class (kept separate from function runtime for scaling/isolation)
- ODGM deployment environment/configuration wiring (collections passed via env/ConfigMap)
- Readiness/health checks for ODGM cluster/collections

## Function addon awareness (env injection)
- When ODGM addon is enabled, function runtimes receive:
 Sample DeploymentRecord:
  - ODGM_SERVICE="<class-name>-odgm-svc:<port>" (default port may be 8081)
  - ODGM_COLLECTIONS="col1,col2,..." (if provided via spec.odgm_config)
- ODGM Deployment also receives ODGM_COLLECTIONS for consistent configuration.

## Addons (simple model; pluggable design)
- spec.addons is a simple list for now (e.g., ["odgm"]).
- ODGM is currently required in practice; the system is designed to add more addons later without changing template selection.
- Addons render their own resources (Deployments/Services) and may inject discovery/config into function pods.

## Observability & security
- Metrics exporter (`/metrics`) over HTTP [planned], or native gRPC metrics interceptor [explore]
- Structured tracing with correlation IDs
- RBAC least privilege, namespace scoping
## Milestones (implementation plan)

1) Foundation (M1)
- [x] Config + tracing
- [x] CRD type stubs
 - [x] Profile → defaults mapping
 - [x] CRD YAMLs in `k8s/crds/`
 - [x] Default Kubernetes namespace wiring via `OPRC_CRM_K8S_NAMESPACE`
 - [x] RBAC manifest for CRM
 - [x] CRD generator binary (`crdgen`) and documented usage

2) Minimal gRPC API + Reconcile (M2)
- [x] Single-port Axum + tonic (gRPC + HTTP on `HTTP_PORT`) with reflection + health
- [x] Implement `deployment.DeploymentService` (initial stubs: Deploy/Status/Delete) using `oprc-grpc` types
- [x] SSA apply of Deployment/Service, owner refs, finalizer (basic)
- [~] Conditions + Events — Conditions: basic Progressing added; Events: emission deferred
- [x] Honor PM contract: idempotency (by `deployment_id`) and correlation id propagation
- [x] Deadline handling (`grpc-timeout`) and basic error code mapping (NOT_FOUND, ALREADY_EXISTS, INVALID_ARGUMENT)
- [x] Map CRD status into structured GetDeploymentStatus payload

3) Templates + ODGM (M3)
- [x] TemplateManager with registry and multi-resource generation
- [x] ODGM Deployment wiring behind flag + function env injection (ODGM_SERVICE, ODGM_COLLECTIONS)
- [x] Environment-aware selection via template scoring
- [x] NFR scoring fallback documented
- [x] Knative support via a dedicated KnativeTemplate with Prometheus monitoring annotations/targets
- [x] Detect Knative availability at CRM startup (CRD presence) and enable template automatically unless disabled

4) NFR Observe-only (M4)
- [x] Analyzer + metrics provider abstraction (operator-only for now)
- [x] Prometheus Operator provider with ServiceMonitor/PodMonitor and PromQL queries
- [x] Recommendations in status (per function/ODGM)
- [x] Switch Analyzer from instant to windowed PromQL range queries using OPRC_CRM_PROM_RANGE and OPRC_CRM_PROM_STEP; aggregate (avg/max) across the window to derive replicas/cpu/memory recommendations; instant query fallback when range is unavailable.
- [ ] Optional: OpenTelemetry metrics provider [planned]

5) Enforcement (M5)
- [x] Basic enforcement (replicas/HPA bounds) with safeguards
- [x] Stability time: require stable recommendations for a configured duration before applying
- [ ] Profile tuning for dev/edge/full
- [ ] Memory usage monitoring and auto-tuning of container requests/limits for function runtimes and ODGM
  - See docs/NFR_ENFORCEMENT_DESIGN.md (Memory tuning) for the proposed approach and safeguards

6) Production hardening (M6)
- [ ] Leader election, concurrency limits
- [ ] gRPC TLS/mTLS, RBAC manifests, network policies
- [ ] Metrics exporter and dashboards
- [ ] gRPC+HTTP shared-port mux (h2c)

## Testing strategy
- Unit: config mapping, profile defaults, template selection, NFR math
- Integration: envtest/kind; CRDs install; SSA patches; Conditions and Events
- E2E (optional): Prometheus stub; observe-only and basic enforcement

Cross-service (PM ↔ CRM):
- Deploy/Status/Delete happy path using a PM stub client against CRM
- Idempotent re-deploy with same `deployment_id` yields stable status
- Error mapping assertions for INVALID_ARGUMENT, NOT_FOUND, UNAVAILABLE

## Local dev
- Build: `cargo build -r`
- Run: `HTTP_PORT=8088 cargo run -p oprc-crm`
- Optional HTTP `/healthz` may remain for convenience during early stages
- Logs: `RUST_LOG=debug`

## Directory structure (target)
```
src/
  config/    # config & profiles
  crd/       # CRD Rust types
  controller/# reconcile logic
  grpc/      # gRPC server + handlers (DeploymentService, Health)
  templates/ # resource generation (planned)
  nfr/       # observe/enforce (planned)
  odgm/      # ODGM integration (planned)
k8s/
  crds/      # CRD YAMLs
  rbac/      # RBAC manifests
  deploy/    # CRM deployment
```

## References
- [Class Runtime Manager Architecture](../../docs/CLASS_RUNTIME_MANAGER_ARCHITECTURE.md)
- [NFR Enforcement Design](../../docs/NFR_ENFORCEMENT_DESIGN.md)
- [NFR Enforcement Design (M5)](../../docs/NFR_ENFORCEMENT_DESIGN.md)
- [Class Runtime Manager (overview)](../../docs/CLASS_RUNTIME_MANAGER.md)
- [NFR Enforcement Design](../../docs/NFR_ENFORCEMENT_DESIGN.md)
- [Package Manager Architecture](../../docs/PACKAGE_MANAGER_ARCHITECTURE.md)
- [Package Manager (overview)](../../docs/PACKAGE_MANAGER.md)
- [Shared Modules Architecture](../../docs/SHARED_MODULES_ARCHITECTURE.md)
