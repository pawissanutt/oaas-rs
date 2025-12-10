# OpenTelemetry Integration

> **Status**: Phase 1–2, 4–5 Complete; Phase 3 In Progress
> **Owners**: CRM / Data Plane / Runtime maintainers
> **Last Updated**: January 2025

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

Current implementation: **external mode** with cluster-level configuration via CRM env vars. Sidecar/deployment modes remain future enhancements.

## 4. Current Configuration Model

### Cluster-Level (CRM Config)

Cluster-level defaults are configured via CRM environment variables:

```rust
// control-plane/oprc-crm/src/config/types.rs
pub struct OtelConfig {
    #[envconfig(from = "OPRC_CRM_OTEL_ENABLED", default = "false")]
    pub enabled: bool,
    #[envconfig(from = "OPRC_CRM_OTEL_ENDPOINT", default = "http://otel-collector:4317")]
    pub endpoint: String,
}
```

### Per-ClassRuntime CRD Fields (Phase 5 — Implemented)

Each ClassRuntime can override cluster defaults using `spec.telemetry`:

```yaml
# k8s/crds/classruntimes.gen.yaml (generated from TelemetrySpec)
telemetry:
  enabled: bool          # Enable/disable OTEL for this CR (overrides cluster)
  traces: bool           # Enable trace export (default: true)
  metrics: bool          # Enable metrics export (default: true)
  logs: bool             # Enable log export (default: false, high overhead)
  sampling_rate: f64     # 0.0–1.0 (default: 0.1 = 10%)
  service_name: string   # Override auto-generated service name
  resource_attributes:   # Custom OTEL resource attributes
    key: value
```

### Merge Logic

CR-level `telemetry` **overrides** cluster defaults with this priority:
1. If `spec.telemetry` is present and `enabled` is set, use CR value
2. Otherwise, fall back to `OPRC_CRM_OTEL_ENABLED` / `OPRC_CRM_OTEL_ENDPOINT`
3. Sub-fields (traces, metrics, logs, sampling_rate, etc.) merge independently

### Status Conditions

The reconciler sets telemetry-specific conditions:
- `TelemetryConfigured` — Telemetry successfully configured (True/False)
- `TelemetryDegraded` — Telemetry enabled but misconfigured (e.g., no endpoint)

## 5. CRM Responsibilities

1. Parse `spec.observability`; short‑circuit if absent or disabled.
2. Render collector assets (ConfigMap + Deployment) when `mode=deployment` and `endpoint` not explicitly supplied.
3. Inject env vars into *all* function containers + ODGM container (if enabled). (Knative Service: inject into template spec.)
4. Sidecar insertion when `mode=sidecar` and `endpoint` not preset.
5. Maintain idempotent SSA patches (owner labels: `oaas.io/owner=<class>` + `component=otel`).
6. Add status conditions; avoid reconciler hot loops (only patch when material change).
7. Respect feature flag `OPRC_CRM_FEATURES_OTEL` (global off switch) and default sampling rate env override.

## 6. Environment Variables

### CRM-Injected Variables (into managed pods)

When `OPRC_CRM_OTEL_ENABLED=true`, CRM injects:

| Var | Source | Example |
|-----|--------|--------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `OPRC_CRM_OTEL_ENDPOINT` | `http://otel-collector:4317` |
| `OTEL_SERVICE_NAME` | Derived from class/function name | `hello-class-fn` |
| `OTEL_LOGS_ENABLED` | Hardcoded `false` (high overhead) | `false` |

### Application-Level Variables (read by `oprc-observability`)

Services using `oprc_observability::setup_tracing()` or `init_otlp_*_if_configured()` read:

| Var | Purpose | Default |
|-----|---------|--------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | General OTLP endpoint | (none—disables export) |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | Traces-specific endpoint override | Falls back to general |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | Metrics-specific endpoint override | Falls back to general |
| `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` | Logs-specific endpoint override | Falls back to general |
| `OTEL_SERVICE_NAME` | Service name attribute | `oaas-service` |
| `OTEL_TRACES_SAMPLER` | Sampler type | `always_on` |
| `OTEL_TRACES_SAMPLER_ARG` | Ratio for `traceidratio` sampler | `0.1` |
| `OTEL_TRACES_ENABLED` | Toggle trace export | `true` |
| `OTEL_LOGS_ENABLED` | Toggle log export | `true` |
| `OTEL_METRICS_PERIOD_SECS` | Metrics export interval | `30` |

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

## 8. Implementation Status

### Completed Components

| Component | Location | Description |
|-----------|----------|-------------|
| **oprc-observability crate** | `commons/oprc-observability/` | Core observability library |
| `TracingConfig` + `setup_tracing()` | `src/tracing.rs` | Unified tracing setup with OTLP traces + logs |
| `init_otlp_tracing()` | `src/otel_exporter.rs` | OTLP trace exporter with configurable sampler |
| `init_otlp_metrics()` | `src/otel_exporter.rs` | OTLP metrics exporter with periodic reader |
| `init_otlp_logs()` | `src/otel_exporter.rs` | OTLP logs exporter via `opentelemetry-appender-tracing` |
| `*_if_configured()` variants | `src/otel_exporter.rs` | Env-driven conditional initialization |
| `OtelMetrics` + `otel_metrics_middleware` | `src/axum_layer.rs` | Axum HTTP metrics middleware |
| `ServiceMetrics` | `src/metrics.rs` | Generic service metrics (requests, duration, errors) |
| `OdgmEventMetrics` | `src/metrics.rs` | ODGM data plane event metrics |
| **oprc-grpc tracing** | `commons/oprc-grpc/src/tracing.rs` | gRPC trace context propagation |
| `inject_trace_context()` | `src/tracing.rs` | W3C TraceContext injection for gRPC clients |
| `OtelGrpcServerLayer` | Re-export from `tonic-tracing-opentelemetry` | Server-side trace extraction |
| **CRM OtelConfig** | `control-plane/oprc-crm/src/config/types.rs` | Cluster-level OTEL config |
| CRM env injection | `src/templates/manager.rs`, `src/templates/odgm.rs` | Inject OTEL envs into pods |
| **ODGM OTEL init** | `data-plane/oprc-odgm/src/main.rs` | Tracing + metrics initialization |
| **Gateway metrics** | `data-plane/oprc-gateway/src/handler/mod.rs` | HTTP metrics middleware |
| **PM metrics** | `control-plane/oprc-pm/src/server.rs` | HTTP metrics middleware |
| **Helm observability stack** | `k8s/charts/deploy-observability.sh` | VictoriaMetrics + OTEL Collector + Grafana |

### Feature Flags

| Crate | Feature | Purpose |
|-------|---------|--------|
| `oprc-observability` | `otlp` (default) | Enable OTLP exporters |
| `oprc-observability` | `axum` | Enable Axum middleware |
| `oprc-grpc` | `otel` | Enable gRPC trace propagation |
| `oprc-crm` | `otel` (default) | Enable OTEL in CRM |

## 9. Phased Delivery

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Env injection (cluster-level config) | ✅ **Complete** — CRM injects OTEL envs when `OPRC_CRM_OTEL_ENABLED=true` |
| 2 | OTLP exporters (traces, metrics, logs) | ✅ **Complete** — `oprc-observability` provides full OTLP support |
| 3 | ODGM + Gateway instrumentation | 🔄 **In Progress** — Basic metrics middleware done; span instrumentation partial |
| 4 | gRPC distributed tracing | ✅ **Complete** — `oprc-grpc` has `inject_trace_context()` + server layer |
| 5 | Per-ClassRuntime CRD config | ✅ **Complete** — `spec.telemetry` field with CR-level override of cluster defaults |
| 6 | Sidecar/deployment collector modes | ❌ **Not Started** — External collector mode only |
| 7 | Metrics alignment + exemplars | ❌ **Not Started** |

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
```yaml
apiVersion: oaas.io/v1alpha1
kind: ClassRuntime
metadata:
  name: example
spec:
  functions:
    - function_key: echo
      container_image: ghcr.io/acme/echo:latest
  telemetry:
    enabled: true
    traces: true
    metrics: true
    logs: false
    sampling_rate: 0.05
    service_name: "my-custom-service"
    resource_attributes:
      team: core
      environment: staging
```

Note: The `collector` block (sidecar/deployment modes) is not yet implemented — Phase 6.

## 16. Testing Strategy
* Unit: CRD parsing (serde defaults), config generator snapshot tests.
* Integration: Reconcile with enabled telemetry → assert Deployment has env vars + sidecar container.
* E2E (optional): Deploy Jaeger all-in-one via kind; send sample function request; verify trace depth >=3 spans.

## 17. Zenoh Trace Context Propagation Design

### Overview

To achieve end-to-end distributed tracing across Gateway → Router → Function → ODGM, we need to propagate W3C TraceContext through Zenoh's messaging layer. Unlike HTTP/gRPC which have standard header mechanisms, Zenoh requires a custom carrier implementation using **attachments**.

### Zenoh Attachment Mechanism

Zenoh supports arbitrary key-value attachments on messages. We use this to carry trace context:

```
┌─────────────────────────────────────────────────────────────┐
│                    Zenoh Message                            │
├─────────────────────────────────────────────────────────────┤
│  Payload: [serialized request/response]                     │
│  Attachments:                                               │
│    - traceparent: "00-<trace_id>-<span_id>-<flags>"        │
│    - tracestate: "oaas=<state>"  (optional)                │
│    - baggage: "key1=val1,key2=val2" (optional)             │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Components

#### 1. Zenoh Trace Carrier (`commons/oprc-zrpc/src/tracing.rs`)

```rust
use opentelemetry::propagation::{Extractor, Injector};
use zenoh::bytes::ZBytes;
use std::collections::HashMap;

/// Carrier for injecting/extracting trace context into Zenoh attachments.
pub struct ZenohTraceCarrier {
    values: HashMap<String, String>,
}

impl ZenohTraceCarrier {
    pub fn new() -> Self {
        Self { values: HashMap::new() }
    }

    /// Create from Zenoh attachment bytes
    pub fn from_attachment(attachment: Option<&ZBytes>) -> Self {
        let mut carrier = Self::new();
        if let Some(bytes) = attachment {
            // Deserialize attachment map (bincode or simple format)
            if let Ok(map) = Self::decode_attachment(bytes) {
                carrier.values = map;
            }
        }
        carrier
    }

    /// Serialize to Zenoh attachment bytes
    pub fn to_attachment(&self) -> Option<ZBytes> {
        if self.values.is_empty() {
            None
        } else {
            Self::encode_attachment(&self.values).ok()
        }
    }

    fn decode_attachment(bytes: &ZBytes) -> Result<HashMap<String, String>, _> { ... }
    fn encode_attachment(map: &HashMap<String, String>) -> Result<ZBytes, _> { ... }
}

impl Injector for ZenohTraceCarrier {
    fn set(&mut self, key: &str, value: String) {
        self.values.insert(key.to_lowercase(), value);
    }
}

impl Extractor for ZenohTraceCarrier {
    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(&key.to_lowercase()).map(|s| s.as_str())
    }
    fn keys(&self) -> Vec<&str> {
        self.values.keys().map(|s| s.as_str()).collect()
    }
}
```

#### 2. Client-Side Injection (ZRPC Client)

Modify `ZrpcClient::call()` to inject trace context:

```rust
// commons/oprc-zrpc/src/client.rs
use crate::tracing::{ZenohTraceCarrier, inject_zenoh_trace_context};

impl<C> ZrpcClient<C> where C: ZrpcTypeConfig {
    pub async fn call(&self, payload: &C::In) -> Result<C::Out, ZrpcError<C::Err>> {
        let byte = C::InSerde::to_zbyte(&payload)?;

        // Inject current trace context into attachment
        let attachment = inject_zenoh_trace_context();

        let (tx, rx) = flume::bounded(self.config.channel_size);
        let mut query = self.z_session
            .get(self.key_expr.clone())
            .payload(byte)
            .target(self.config.target)
            // ... other config ...

        // Add trace context attachment if present
        if let Some(att) = attachment {
            query = query.attachment(att);
        }

        query.callback(move |s| { let _ = tx.send(s); }).await?;
        // ... handle response ...
    }
}

/// Inject current span's trace context into a Zenoh attachment.
pub fn inject_zenoh_trace_context() -> Option<ZBytes> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let span = tracing::Span::current();
    let context = span.context();

    let mut carrier = ZenohTraceCarrier::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut carrier);
    });
    carrier.to_attachment()
}
```

#### 3. Server-Side Extraction (ZRPC Server)

Modify `ZrpcService::handle()` to extract trace context and create child spans:

```rust
// commons/oprc-zrpc/src/server/sync.rs
use crate::tracing::{ZenohTraceCarrier, extract_zenoh_trace_context};

impl<T, C> ZrpcService<T, C> {
    async fn handle(handler: &Arc<T>, query: Query, config: &ServerConfig) {
        // Extract trace context from query attachment
        let parent_context = extract_zenoh_trace_context(query.attachment());

        // Create span with extracted parent context
        let span = tracing::info_span!(
            "zrpc.server.handle",
            otel.kind = "server",
            rpc.service = %config.service_id,
            rpc.method = query.key_expr().as_str(),
        );

        // Link span to parent context if present
        if let Some(ctx) = parent_context {
            span.set_parent(ctx);
        }

        let _guard = span.enter();

        // Decode and handle request...
        let payload = query.payload().expect("Payload expected");
        // ... existing handler logic ...
    }
}

/// Extract trace context from Zenoh attachment and return OpenTelemetry context.
pub fn extract_zenoh_trace_context(attachment: Option<&ZBytes>) -> Option<opentelemetry::Context> {
    let carrier = ZenohTraceCarrier::from_attachment(attachment);
    let context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&carrier)
    });
    // Return Some only if context has valid trace info
    if context.span().span_context().is_valid() {
        Some(context)
    } else {
        None
    }
}
```

### Attachment Encoding Format

For minimal overhead, use a simple length-prefixed binary format:

```
┌────────┬─────────────────┬────────────────────┬───────┐
│ count  │ key1_len + key1 │ val1_len + val1    │ ...   │
│ (u8)   │ (u16 + bytes)   │ (u16 + bytes)      │       │
└────────┴─────────────────┴────────────────────┴───────┘
```

Alternative: Use existing `bincode` serialization of `HashMap<String, String>`.

### Router Pub/Sub Context Propagation

For Router's Zenoh pub/sub (not query/reply), propagate context via publication attachments:

```rust
// data-plane/oprc-router/src/publisher.rs
pub async fn publish_with_trace(&self, key: &str, payload: ZBytes) -> Result<()> {
    let attachment = inject_zenoh_trace_context();
    let mut pub_builder = self.session.put(key, payload);
    if let Some(att) = attachment {
        pub_builder = pub_builder.attachment(att);
    }
    pub_builder.await
}

// Subscriber side
pub async fn on_sample(&self, sample: Sample) {
    let parent_ctx = extract_zenoh_trace_context(sample.attachment());
    let span = tracing::info_span!("zenoh.subscriber", topic = sample.key_expr().as_str());
    if let Some(ctx) = parent_ctx {
        span.set_parent(ctx);
    }
    // Process message...
}
```

### Feature Flag

Add `otel` feature to `oprc-zrpc`:

```toml
# commons/oprc-zrpc/Cargo.toml
[features]
otel = ["dep:opentelemetry", "dep:tracing-opentelemetry"]

[dependencies]
opentelemetry = { workspace = true, optional = true }
tracing-opentelemetry = { workspace = true, optional = true }
```

When disabled, tracing functions become no-ops.

### Trace Flow Example

```
Gateway (HTTP)                    Router (Zenoh)                 Function (ZRPC)
     │                                 │                              │
     │ ─── POST /invoke ──────────────▶│                              │
     │     traceparent: 00-abc-111-01  │                              │
     │                                 │                              │
     │                                 │ ─── Zenoh Query ────────────▶│
     │                                 │     attachment:               │
     │                                 │       traceparent: 00-abc-222-01
     │                                 │                              │
     │                                 │                    ┌─────────┴─────────┐
     │                                 │                    │ span: zrpc.handle │
     │                                 │                    │ parent: abc-222   │
     │                                 │                    └─────────┬─────────┘
     │                                 │                              │
     │                                 │◀─── Zenoh Reply ─────────────│
     │                                 │     attachment:               │
     │                                 │       traceparent: 00-abc-333-01
     │◀────────────────────────────────│                              │
     │                                 │                              │

Resulting Trace:
  [abc-111] gateway.http.request
    └── [abc-222] router.forward
          └── [abc-333] zrpc.server.handle
                └── [abc-444] function.execute
```

### Performance Considerations

1. **Attachment overhead**: ~60-80 bytes per message for traceparent + tracestate
2. **Sampling**: Use `ParentBased` sampler so child spans respect parent's sampling decision
3. **Conditional propagation**: Skip attachment when OTEL is disabled or span is not sampled:
   ```rust
   pub fn inject_zenoh_trace_context() -> Option<ZBytes> {
       let span = tracing::Span::current();
       if !span.is_sampled() {
           return None;
       }
       // ... inject ...
   }
   ```

### Migration Path

1. **Phase 1**: Add `ZenohTraceCarrier` to `oprc-zrpc` behind `otel` feature
2. **Phase 2**: Update `ZrpcClient::call()` to inject trace context
3. **Phase 3**: Update `ZrpcService::handle()` to extract and create child spans
4. **Phase 4**: Add pub/sub propagation to Router

## 18. Remaining Work

### High Priority
1. **Gateway span instrumentation** — Add `#[instrument]` to request handlers with semantic attributes (`http.method`, `http.route`, `oaas.class`, `oaas.function`).
2. **ODGM storage operation spans** — Instrument shard operations (get/put/delete) with duration and key metadata.
3. **Zenoh trace propagation** — Implement `ZenohTraceCarrier` and integrate with ZRPC client/server (see Section 17).

### Medium Priority
4. **Per-ClassRuntime CRD fields** — Add `spec.observability` to CRD schema; update CRM reconcile to read CR-level overrides.
5. **Status conditions** — Add `TelemetryConfigured` / `TelemetryDegraded` conditions to ClassRuntimeStatus.
6. **Grafana dashboards** — Publish pre-built dashboards for request latency, error rates, trace analysis.

### Low Priority / Future
7. **Sidecar collector mode** — CRM-managed per-pod OTEL Collector sidecar injection.
8. **Deployment collector mode** — Per-class shared collector Deployment.
9. **Adaptive sampling** — Dynamic sampling rate based on error rate or load.

### TODO (Deferred)
- [ ] **Sampling rate validation** — Clamp `sampling_rate` to 0.0–1.0 range; log warning if >0.5 (high overhead).
- [ ] **Per-CR collector endpoint override** — Allow `spec.telemetry.collector.endpoint` to override cluster-level `OPRC_CRM_OTEL_ENDPOINT`. Currently only central collector mode is supported.

## 19. Observability Stack Deployment

A pre-configured observability stack is available:

```bash
# Deploy VictoriaMetrics + OTEL Collector + Grafana
./k8s/charts/deploy-observability.sh install

# OTEL Collector endpoint (for OPRC_CRM_OTEL_ENDPOINT)
http://otel-collector-opentelemetry-collector.observability.svc.cluster.local:4317
```

Stack components:
- **VictoriaMetrics Single** — Metrics storage (Prometheus-compatible)
- **VictoriaLogs Single** — Log aggregation
- **VictoriaTraces Single** — Trace storage (Jaeger-compatible)
- **OTEL Collector** — Receives OTLP, routes to Victoria backends
- **Grafana** — Visualization (admin/admin)

Configuration: `k8s/charts/otel-collector-values.yaml`

## 20. Usage Examples

### Enable OTEL in CRM
```bash
export OPRC_CRM_OTEL_ENABLED=true
export OPRC_CRM_OTEL_ENDPOINT=http://otel-collector:4317
cargo run -p oprc-crm
```

### Initialize tracing in a service
```rust
use oprc_observability::{TracingConfig, setup_tracing};

let config = TracingConfig::from_env("my-service", "info", true);
setup_tracing(config)?;

// Optionally initialize metrics
oprc_observability::init_otlp_metrics_if_configured("my-service")?;
```

### Inject trace context in gRPC client
```rust
use oprc_grpc::tracing::inject_trace_context;

let mut request = tonic::Request::new(my_message);
inject_trace_context(&mut request);
client.my_rpc(request).await?;
```

---
Document last updated: December 2025
