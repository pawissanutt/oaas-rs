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
- Resulting resources: one or more function Deployments/Services (or Knative Services) plus a separate ODGM Deployment/Service when enabled.
- status fields (controller-owned): Conditions (Available/Progressing/Degraded), phase, message, observedGeneration, resource_refs, nfr_recommendations, odgm status

Flow summaries
1) Deploy
  - PM → CRM Deploy: upsert CRD, ensure finalizer, return `accepted=true` and a status snapshot
  - Controller reconciles: SSA apply resources for the Class: function Deployments/Services (or Knative) and an ODGM Deployment/Service (separate workload), set ownerRefs, update Conditions/Events
2) Status
  - PM → CRM GetDeploymentStatus: CRM reads CRD; maps Conditions/phase and returns resource refs and recommendations
3) Delete
  - PM → CRM DeleteDeployment: CRM sets deletion (or ensures finalizer), controller deletes children, removes finalizer; subsequent status → NOT_FOUND

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
- Tracing: include `deployment_id` and `x-correlation-id` in all logs and Events
- Metrics: controller reconcile durations, queue depth, errors; optional Prometheus URL for NFR observe/enforce

## Current status (scaffold)
- kube-rs controller loop for `DeploymentRecord` with basic SSA apply, owner refs, and finalizer handling
- Env-based config (`OPRC_CRM_*`) and tracing setup
- gRPC server on `GRPC_PORT` with Health and Reflection; `DeploymentService` implements Deploy/Status/Delete using `commons/oprc-grpc`
- Default Kubernetes namespace via `OPRC_CRM_K8S_NAMESPACE`; correlation id is propagated to CRD annotations and echoed in responses
- Conditions use enums (Available/Progressing/Degraded/Unknown) instead of string matching; status summary derived from conditions
- gRPC deadlines respected via `grpc-timeout` header; K8s calls are wrapped with per-RPC timeouts
- Idempotency enforced: name collisions with different `oaas.io/deployment-id` return ALREADY_EXISTS
- Optional HTTP `/healthz` stub remains for local convenience

## Scope and non-goals
- Scope: Kubernetes-only, single-cluster controller; HTTP API for PM ↔ CRM; CRDs as source of truth; optional NFR observe/enforce; optional ODGM (as a separate Deployment).
- Non-goals: Multi-cluster orchestration, alternate backends (Docker Compose), non-Kubernetes runtimes.

## Configuration (env)
- `OPRC_CRM_PROFILE` dev|edge|full (default dev)
- `GRPC_PORT` (default 7088) – primary interface
- `HTTP_PORT` (optional) – may be used for `/metrics` later
- `OPRC_CRM_FEATURES_NFR_ENFORCEMENT` (false)
- `OPRC_CRM_FEATURES_HPA` (false)
- `OPRC_CRM_FEATURES_KNATIVE` (false)
- `OPRC_CRM_FEATURES_PROMETHEUS` (false)
- `OPRC_CRM_FEATURES_LEADER_ELECTION` (true)
- `OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS` (120)
- `OPRC_CRM_ENFORCEMENT_MAX_REPLICA_DELTA` (30)
- `OPRC_CRM_LIMITS_MAX_REPLICAS` (20)

Additional:
- `OPRC_CRM_K8S_NAMESPACE` — default namespace (auto-detected in-cluster if unset)
- `OPRC_CRM_PROM_URL` — Prometheus base URL (optional in dev/edge)
- `OPRC_CRM_SECURITY_MTLS` — enable gRPC mTLS (default false in dev, true in full)
- `SERVICE_LOG` — log level (`debug` in dev by default)

Planned: TLS/mTLS flags hardening and related security knobs.

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
  - spec:
    - deployment_unit, function_containers
    - target_environment
    - nfr_requirements (e.g., min_throughput_rps, max_latency_ms, availability)
    - template_hint: optional template selector (e.g., "dev", "edge", "cloud")
    - addons: optional string list for requested addons (simple for now), e.g., ["odgm"]
    - odgm_config: optional, currently collection-focused for replication topology
      - collections: ["orders", "users", ...]
    - resource_allocation (optional)
  - status: Conditions (Available/Progressing/Degraded), phase, message, observedGeneration, resource_refs, nfr_compliance, odgm status
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
- Emit Kubernetes Events for major transitions
- Conservative requeues/backoff; optional leader election

## Template system
- TemplateManager as a registry of multiple templates (e.g., Dev, Edge, Cloud; extensible)
- Selection: spec.template_hint → NFR heuristic (min_throughput_rps, latency, availability) → profile default
- K8s Deployment/Service generation (first target) and optional Knative (behind feature)
- ODGM Deployment wiring behind a flag (ODGM is not a sidecar)

## NFR analysis & enforcement
- Modes: off (default in dev), basic (edge), full (prod)
- Observe-only first (populate recommendations in status)
- Prometheus-based monitoring (optional in dev/edge)
- Safeguards: cooldown, hysteresis, bounded delta, max replicas

## ODGM integration
- Config generation (collections; replication is network-abstracted)
- Separate ODGM Deployment + Service for the Class
- ODGM deployment environment/configuration wiring (collections passed via env/ConfigMap)
- Readiness/health checks for ODGM cluster/collections

## Function addon awareness (env injection)
- When ODGM addon is enabled, function runtimes receive:
  - ODGM_ENABLED=true
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
- gRPC TLS/mTLS [planned]

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
- [x] Tonic server on `GRPC_PORT` with reflection + health
- [x] Implement `deployment.DeploymentService` (initial stubs: Deploy/Status/Delete) using `oprc-grpc` types
 - [x] SSA apply of Deployment/Service, owner refs, finalizer (basic)
 - [~] Conditions + Events — Conditions: basic Progressing added; Events: emission deferred
 - [x] Honor PM contract: idempotency (by `deployment_id`) and correlation id propagation
 - [x] Deadline handling (`grpc-timeout`) and basic error code mapping (NOT_FOUND, ALREADY_EXISTS, INVALID_ARGUMENT)
 - [x] Map CRD status into structured GetDeploymentStatus payload

3) Templates + ODGM (M3)
- [ ] TemplateManager with multi-template registry and multi-resource generation (function Deployments/Services and ODGM Deployment/Service)
- [ ] ODGM Deployment wiring behind flag + function env injection (ODGM_SERVICE, ODGM_COLLECTIONS)
- [ ] Basic template selection by NFR heuristics (throughput/latency/availability) with template_hint override

4) NFR Observe-only (M4)
- [ ] Analyzer + Prometheus integration (optional)
- [ ] Recommendations in status

5) Enforcement (M5)
- [ ] Basic enforcement (replicas/HPA bounds) with safeguards
- [ ] Profile tuning for dev/edge/full

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
- Run: `GRPC_PORT=7088 cargo run -p oprc-crm`
- Optional HTTP `/healthz` may remain for convenience during early stages
- Logs: `SERVICE_LOG=debug`

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
- [Class Runtime Manager (overview)](../../docs/CLASS_RUNTIME_MANAGER.md)
- [Package Manager Architecture](../../docs/PACKAGE_MANAGER_ARCHITECTURE.md)
- [Package Manager (overview)](../../docs/PACKAGE_MANAGER.md)
- [Shared Modules Architecture](../../docs/SHARED_MODULES_ARCHITECTURE.md)
