# Memory Usage Monitoring and Auto-Tuning (Draft)

Goal: Observe real memory usage for OaaS function runtimes and ODGM, then automatically recommend (observe-only) and later enforce tuned container memory requests/limits to minimize OOMs and reduce overprovisioning.

## Scope and success criteria
- Phase 1 (observe-only):
  - Collect per-pod memory usage time series and peaks.
  - Compare against current requests/limits; compute recommendations with safety buffers.
  - Surface recommendations in DeploymentRecord status.
- Phase 2 (enforcement):
  - Optionally patch workloads (or HPA/VPA integration) with tuned values under safeguards.

Success metrics:
- Reduce OOMKill rate and throttling.
- Reduce average over-request ratio while keeping headroom for P95/99.

## Enablement and overrides
- Global feature flags
  - Observe-only: enabled by default when Prometheus URL is configured.
  - Enforcement is gated by `OPRC_CRM_FEATURES_NFR_ENFORCEMENT=true` (off by default).
- Per-deployment controls (proposed CRD fields)
  - `spec.nfr.memory_tuning.enabled: bool` — enable/disable memory tuning for this DeploymentRecord (default: true in edge/full, false in dev).
  - `spec.nfr.memory_tuning.enforce: bool` — allow automatic patches for this DeploymentRecord (default: false).
  - `spec.nfr.memory_tuning.overrides: map<string, { request: string, limit: string }>` — explicit per-container values (e.g., `{"function": {request: "256Mi", limit: "512Mi"}}`) that take precedence over recommendations and enforcement.
- Precedence
  1) Explicit overrides (per-deployment) >
  2) Enforcement-applied values >
  3) Recommendations (status only) >
  4) Template defaults.

## Data sources
- Kubernetes Metrics via Prometheus:
  - container_memory_working_set_bytes
  - container_memory_rss
  - kube_pod_container_resource_requests{resource="memory"}
  - kube_pod_container_resource_limits{resource="memory"}
- Optional: cAdvisor or node exporter for corroboration.

## Collection strategy
- Identify targets by label: oaas.io/owner=<class-name>.
- Query Prometheus for the following windows:
  - Short window (5-15m) for responsiveness.
  - Long window (1-24h) for stability.
- Aggregate per-container:
  - max(working_set), p95(working_set), avg(working_set).
- Align with requested/limit values via kube-state-metrics series.

## Recommendation algorithm (initial)
Inputs per container:
- ws_max, ws_p95, ws_avg over windows.
- current_request_mem, current_limit_mem.

Rules:
- Recommended request = max(ws_p95 * 1.20, ws_avg * 1.50, 64Mi), rounded up to nearest 16Mi.
- Recommended limit = max(ws_max * 1.10, recommended_request * 1.50), rounded up to nearest 16Mi.
- Minimum/maximum guardrails: clamp request to [64Mi, 4Gi] configurable; clamp limit to [128Mi, 8Gi].
- Hysteresis: only propose changes if delta > 20% or absolute change > 64Mi.
- Cooldown: don’t revise same container more than once per 30m.

## Concurrency-aware recommendations (Knative only)
When Knative is enabled, memory usage and per-pod concurrency are tightly coupled. Knative can enforce hard concurrency via `spec.template.spec.containerConcurrency` and soft targets via autoscaling annotations.

Signals and data:
- Knative autoscaler metrics (via Prometheus):
  - `queue_depth`, `request_concurrency`, `request_count` (names depend on setup)
  - Revision-level series for concurrency/queue
- Current Knative settings:
  - `spec.template.spec.containerConcurrency` (hard limit; 0 means unlimited)
  - Annotations on the Revision template:
    - `autoscaling.knative.dev/metric` (e.g., "concurrency")
    - `autoscaling.knative.dev/target` (soft target concurrency per pod)
    - `autoscaling.knative.dev/targetBurstCapacity` (buffer)

Guidelines:
- If `ws_max` is consistently near limit AND `request_concurrency_p95` > target:
  - Prefer lowering effective concurrency to reduce per-pod memory spikes:
    - Reduce `target` by 10–25% (not below 1)
    - Optionally set/adjust `containerConcurrency` to a finite bound if currently unlimited
- If `ws_max` is far from limit AND `queue_depth_p95` is high while `request_concurrency_p95` is below target:
  - Consider increasing soft `target` modestly (≤20%) to utilize memory headroom
- Always pair concurrency changes with memory recommendations to avoid oscillation; apply hysteresis and cooldown similar to memory rules.

Enforcement (if enabled):
- Patch Knative Service template annotations and `containerConcurrency` via SSA.
- Never override explicit user settings for `containerConcurrency` or autoscaling annotations if provided as overrides.
- Per-deployment overrides (proposed): `spec.nfr.knative_tuning.overrides.containerConcurrency` and `spec.nfr.knative_tuning.overrides.annotations.*`.

## Presentation
- Place recommendations under DeploymentRecord.status.nfr_recommendations.memory for each container:
```
containers:
- name: function
  current:
    request: 256Mi
    limit: 512Mi
  observed:
    ws_avg: 110Mi
    ws_p95: 180Mi
    ws_max: 210Mi
    window: 6h
  recommended:
    request: 216Mi
    limit: 324Mi
  rationale: "p95-based with 20% headroom; rounded to 224Mi and 336Mi"
```
- Also emit a Kubernetes Event when a new recommendation is produced.

## Enforcement (later)
- Feature flag `OPRC_CRM_FEATURES_NFR_ENFORCEMENT=true` gates writes.
- Apply via SSA patch to Deployment/KnativeService templates, or prefer VPA integration if enabled.
- Safeguards:
  - bounded delta per update (e.g., 30%)
  - cooldown between changes
  - never decrease below observed p95
 - Never override user-provided per-deployment overrides in `spec.nfr.memory_tuning.overrides`.

## Knative considerations
- When Knative is enabled, patch memory resources on the Knative Service container spec.
- Concurrency knobs:
  - Hard limit: `spec.template.spec.containerConcurrency`
  - Soft target: `autoscaling.knative.dev/target` (with `autoscaling.knative.dev/metric=concurrency`)
  - Burst capacity: `autoscaling.knative.dev/targetBurstCapacity`
- Ensure Prometheus has scrape configs for Knative pods; use same label owner to select targets.

## Implementation notes
- Add a Prometheus client module in CRM (configurable base URL via OPRC_CRM_PROM_URL).
- Analyzer job runs on a schedule in the controller loop or a separate worker.
- Unit tests for rounding/clamping/hysteresis logic.

## Open questions
- Should we integrate with VPA directly vs. custom patching?
- How to store historical recommendations for audit? (CRD annotations or external store?)
- Multi-container pods support (sidecars) and distinguishing function vs ODGM.
