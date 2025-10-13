# OpenTelemetry Integration Proposal (ClassRuntime, Functions, ODGM, Gateway, CRM)

> Status: Draft (Phase 0)
> Owners: CRM / Data Plane / Runtime maintainers
> Target Release: Incremental (Phases 1–3)

## 1. Problem & Goals

We need unified, low‑overhead observability (traces, metrics, structured logs) spanning:
* Control plane (CRM decisions, reconciliation latency, enforcement events)
* Data plane (Function request handling, ODGM data operations, Router hops)
* Cross‑component request causality (end‑to‑end trace from external API → PM → CRM (deploy) OR Gateway → Function → ODGM)

Goals (MVP → Advanced):
1. Correlated distributed traces (W3C TraceContext baseline; optional B3) with stable service taxonomy.
2. OTLP (gRPC+HTTP) single abstraction; pluggable exporters (Jaeger / Tempo / OTEL Collector upstream).
3. Metrics alignment (existing Prometheus scraping + OTEL semantic conventions) without duplicating instrumentation cost.
4. Minimal user friction: **CRM auto‑injects** env + (optionally) Collector sidecar or references a shared Deployment/DaemonSet.
5. Configurability per ClassRuntime: enable/disable, sampling rate, resource attributes; cluster defaults via env.
6. Low risk: no mandatory sidecars; graceful degrade if collector absent.

Non-goals (initial phases):
* Custom eBPF network tracing.
* Log aggregation pipeline (we only emit structured logs; shipping optional).
* Adaptive sampling heuristics (future).

## 2. High-Level Architecture

```
Client → Gateway → Function ─┬─> ODGM (state ops)
                             │
                             └─> Router (Zenoh) (future span linking)

PM → CRM (deploy/status/delete)

All instrumented processes export OTLP → (Sidecar | Node (DaemonSet) | Shared Deployment) OTEL Collector → Backends:
    * Traces: Jaeger / Tempo
    * Metrics: Prometheus (scrape collector or direct app metrics already exposed)
    * Logs (optional): Loki / OTLP
```

## 3. Deployment Modes

| Mode | Description | Pros | Cons | When |
|------|-------------|------|------|------|
| sidecar | Per Function/ODGM pod collector sidecar | Isolation, per‑class config | Higher pod cost | Small clusters, dev |
| deployment | One shared collector `<class>-otel-collector` | Lower overhead per app | Per-class Deployment objects | Medium multi‑tenant scenarios |
| daemonset (future) | Node-wide collector | Lowest marginal cost | Requires endpoint discovery & security policies | Large homogeneous clusters |
| external (no manage) | Cluster operator supplies endpoint | Simplest for CRM | Less automation | Managed platforms |

Initial implementation: sidecar + shared deployment. DaemonSet & external: follow‑up.

## 4. CRD Extensions (Proposed)

Add `spec.observability` (all fields optional):
```
observability:
  enabled: bool (default false)
  telemetry:
    traces: bool (default true)
    metrics: bool (default true)
    logs: bool (default false)
    sampling_rate: f64 (0.0–1.0, default 0.1)
    service_name: string (override)
    propagators: ["w3c", "b3"] (default ["w3c","b3"]) ordered → join into OTEL_PROPAGATORS
    resource_attributes: map[string]string (merged with defaults)
  collector:
    mode: sidecar|deployment (default sidecar)
    image: string (default from CRM env)
    endpoint: string (override; if set we skip CRM-managed collector creation)
    exporters:
      traces: string (endpoint or logical name)
      metrics: string
      logs: string
```

Generated condition examples (added to status.conditions):
* `TelemetryConfigured=True` (successful injection)
* `TelemetryDegraded=True` with reason (CollectorUnavailable, ConfigApplyError)

## 5. CRM Responsibilities

1. Parse `spec.observability`; short‑circuit if absent or disabled.
2. Render collector assets (ConfigMap + Deployment) when `mode=deployment` and `endpoint` not explicitly supplied.
3. Inject env vars into *all* function containers + ODGM container (if enabled). (Knative Service: inject into template spec.)
4. Sidecar insertion when `mode=sidecar` and `endpoint` not preset.
5. Maintain idempotent SSA patches (owner labels: `oaas.io/owner=<class>` + `component=otel`).
6. Add status conditions; avoid reconciler hot loops (only patch when material change).
7. Respect feature flag `OPRC_CRM_FEATURES_OTEL` (global off switch) and default sampling rate env override.

## 6. Environment Variables (Injected)

| Var | Source | Example |
|-----|--------|---------|
| OTEL_EXPORTER_OTLP_ENDPOINT | Derived (mode) or `spec.observability.collector.endpoint` | `http://hello-class-otel-collector:4317` |
| OTEL_SERVICE_NAME | `telemetry.service_name` or `<class>-<container>` | `hello-class-fn` |
| OTEL_TRACES_EXPORTER | `otlp` or `none` | `otlp` |
| OTEL_METRICS_EXPORTER | same pattern | `otlp` |
| OTEL_LOGS_EXPORTER | same pattern | `none` |
| OTEL_TRACES_SAMPLER | fixed initial | `parentbased_traceidratio` |
| OTEL_TRACES_SAMPLER_ARG | sampling_rate | `0.1` |
| OTEL_PROPAGATORS | join(propagators) | `tracecontext,baggage,b3` |
| OTEL_RESOURCE_ATTRIBUTES | merged static + CR attributes | `deployment.name=hello-class,team=platform` |

Defaults (cluster level) via CRM env (`OPRC_CRM_OTEL_*`).

## 7. Collector Config Generation (Template)

Generated ConfigMap `(<class>-otel-config)`:
```
receivers:
  otlp:
    protocols:
      grpc:
      http:
processors:
  batch: { send_batch_size: 1024, timeout: 10s }
  probabilistic_sampler: { sampling_percentage: <rate * 100> }
  resource:
    attributes:
      - key: service.namespace
        value: oaas
        action: upsert
      - key: deployment.name
        value: <class>
exporters:
  <dynamic: traces / metrics / logs sections>
service:
  pipelines:
    traces: { receivers: [otlp], processors: [resource, probabilistic_sampler, batch], exporters: [traces] }
    metrics: { receivers: [otlp], processors: [resource, batch], exporters: [metrics] }
    logs:    { receivers: [otlp], processors: [resource, batch], exporters: [logs] }
```
Pipelines omitted when disabled.

## 8. Code Changes (Summary)

| Area | Action |
|------|--------|
| `commons/oprc-models` or CRD module | Add structs for ObservabilityConfig & friends; regenerate CRD YAML |
| CRM reconcile | Detect + pass observability block to template context |
| TemplateManager | Inject env + sidecar; generate ConfigMap + Deployment (collector) as `RenderedResource::Other` |
| Data plane crates (gateway, odgm) | Initialize tracing (feature `otel`) behind runtime env detection |
| Config | Extend `CrmConfig` with `otel` sub-config + feature flag |
| Status | Add conditions & optional resource refs (collector deployment) |
| Docs | This file + README mentions |
| CI (optional) | Add integration test enabling telemetry (dry-run apply) |

## 9. Phased Delivery

| Phase | Scope | Exit Criteria |
|-------|-------|---------------|
| 1 | CRD + env injection (no sidecar deployment) | Traces appear when external collector endpoint set |
| 2 | Sidecar + per-class collector deployment mode | Automated config, status condition updated |
| 3 | ODGM + Gateway instrumentation (spans around RPC / storage ops) | End‑to‑end trace from Gateway → Function → ODGM visible |
| 4 | Metrics alignment (semantic conventions, exemplar linking) | Prometheus shows OTEL metrics; NFR analyzer can query |
| 5 | Advanced (log exporter, daemonset option, adaptive sampling) | Config toggles; documented trade‑offs |

## 10. Security & Multi-Tenancy
* Restrict collector to namespace (RBAC principle of least privilege).
* Owner labels for garbage collection.
* No dynamic code injection; only env var + optional sidecar.
* Potential isolation concern: sidecar mode isolates better, shared deployment shares pipelines; document for users.

## 11. Performance Considerations
* Default 10% sampling to avoid overhead; can be tuned down to 1% in high-RPS production.
* Batch processor w/ 10s or 512–1024 span flush threshold (tunable via env later).
* Avoid double instrumentation (e.g., do not register global OTEL twice) — guard with `OnceCell`.
* Provide build feature flags (`otel`) to allow stripping dependencies.

## 12. Failure & Degradation Modes
| Scenario | Behavior |
|----------|----------|
| Collector absent | App logs warning; tracing layer disabled gracefully |
| Misconfigured endpoint | Connection errors logged (rate limited); sampling still applied locally |
| High backpressure | OTEL SDK drops spans (bounded queue); no crash |
| Disabled in CRD mid-flight | Reconcile removes sidecar in subsequent apply (new pod template hash) |

## 13. Open Questions
1. Merge OTEL metrics with existing custom metrics vs keep separate endpoints? (Initial: keep existing Prom scraping.)
2. Should CRM enforce max sampling rate (prevent 100%)? (Propose clamp 0.0–1.0, warn >0.5.)
3. Do we expose trace IDs in user function responses for correlation? (Future header injection via Gateway.)

## 14. Migration Plan
1. Add CRD fields (backward compatible; all optional). Re-generate CRD YAML and version bump (patch).
2. Ship with feature flag off by default; enable in dev profile examples.
3. Provide demo manifest + quickstart doc (Phase 2).
4. Publish example Grafana dashboard JSON (Phase 4).

## 15. Sample CRD Snippet
```
apiVersion: oaas.io/v1alpha1
kind: ClassRuntime
metadata:
  name: example
spec:
  functions:
    - function_key: echo
      container_image: ghcr.io/acme/echo:latest
  observability:
    enabled: true
    telemetry:
      traces: true
      metrics: true
      logs: false
      sampling_rate: 0.05
      resource_attributes:
        team: core
        environment: staging
    collector:
      mode: sidecar
```

## 16. Testing Strategy
* Unit: CRD parsing (serde defaults), config generator snapshot tests.
* Integration: Reconcile with enabled telemetry → assert Deployment has env vars + sidecar container.
* E2E (optional): Deploy Jaeger all-in-one via kind; send sample function request; verify trace depth >=3 spans.

## 17. Next Steps (Actionable)
1. Implement CRD structs + regenerate CRD YAML.
2. Extend `RenderContext` to carry observability block.
3. Sidecar injection + env var injection helpers.
4. Collector ConfigMap + Deployment renderer.
5. ODGM tracing init (`opentelemetry`, `tracing-opentelemetry`).
6. Gateway/Function instrumentation (HTTP server spans, client spans to ODGM, attributes: deployment_id, function_key, request_id if available).
7. Documentation updates + quickstart.

---
Feedback welcome before Phase 1 code changes.
