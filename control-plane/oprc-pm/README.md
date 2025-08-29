# OaaS Package Manager (PM)

Service that manages OaaS packages and orchestrates class/function deployments across one or more CRM-managed clusters. PM exposes a REST API and talks to each CRM via gRPC (tonic). Storage is pluggable (memory now, etcd planned).

## What it does
- Package registry CRUD (stores `OPackage` from `commons/oprc-models`).
- Deployment orchestration for classes (`OClassDeployment` → per-cluster `DeploymentUnit` [protobuf from `commons/oprc-grpc`]).
- Multi-cluster awareness via a simple CRM manager; default cluster support out of the box.
- Read-through views for Class Runtimes and Status via CRM.

Current constraints
- PM→CRM uses gRPC and the Health service; Deploy/Status/Delete/List/Get are implemented.
- PM sends the protobuf `DeploymentUnit` (from `commons/oprc-grpc`) directly to CRM; no model→proto mapping layer.
- CRM List/Get Class Runtimes return protobuf `DeploymentUnit` items; detailed status is available via `GetDeploymentStatus` (includes `status_resource_refs`).
- Idempotency/correlation IDs and advanced timeouts/retries are basic and will be hardened.
- Storage backends: in-memory is implemented; etcd is stubbed.
 - Availability propagation: PM now prefers CRM's `CrmInfoService::GetClusterHealth` which returns an `availability` field (0..1). When present this powers quorum / consistency aware replica sizing (see "Availability‑driven replica sizing").
 - Schema alignment: function-level `replicas` and `container_image` fields were removed from legacy models; image and autoscaling bounds live in per‑function `ProvisionConfig` (container_image, min_scale, max_scale).

## Architecture at a glance
- HTTP API (Axum) on a single port.
- Services:
  - PackageService — validates and stores packages and optionally schedules deployments.
  - DeploymentService — creates protobuf `DeploymentUnit`s and calls CRM per target cluster; persists `OClassDeployment`.
- CRM Manager — resolves a per-cluster client; supports default cluster selection and health reads.
- Storage — via `commons/oprc-cp-storage` traits; memory backend in use.

Key modules
- `src/server.rs` — Axum routes and server wiring.
- `src/services/{package,deployment}.rs` — business logic.
- `src/crm/{manager,client}.rs` — CRM fanout and gRPC client (tonic) using `commons/oprc-grpc`.
- `src/config/app.rs` — env-first config with helpers for server/storage/CRM/observability.
 - `tests/it_pm_crm.rs` — in‑process and mocked CRM integration tests.

## HTTP API (v1)
Base path: `/api/v1`

Packages
- POST `/packages` → Create package (body: `OPackage`).
- GET `/packages` → List with filters: `name_pattern`, `disabled`, `tags`, `limit`, `offset`.
- GET `/packages/{name}` → Get one.
- POST `/packages/{name}` → Update (name must match body).
- DELETE `/packages/{name}` → Delete.

Deployments
- POST `/deployments` → Create class deployment (body: `OClassDeployment`).
- GET `/deployments` → List deployments (filters: `package_name`, `class_key`, `target_env`, `target_cluster`, `limit`, `offset`).
- GET `/deployments/{key}` → Get one.
- DELETE `/deployments/{key}?cluster={name}` → Delete from a cluster (requires `cluster` query param for now).

Class runtimes and status (proxied to CRM)
- GET `/class-runtimes` → Aggregate across clusters (optional filters: `package_name`, `class_key`, `environment`, `cluster`, `status`, `limit`, `offset`).
- GET `/class-runtimes/{id}` → Fetch by id (searches specified cluster or all clusters).
- Aliases preserved: `/deployment-records`, `/deployment-records/{id}`.
- GET `/deployment-status/{id}` → Current status (default cluster or `?cluster=`).

Clusters and catalog
- GET `/clusters` → Known clusters.
- GET `/clusters/health` → Health across all configured clusters (aggregated).
- GET `/clusters/{name}/health` → CRM health for a cluster.
- GET `/classes` → All classes across packages.
- GET `/functions` → All functions across packages.

Health
- GET `/health`.

Error model
- JSON body `{ "error": "..." }` with HTTP codes: 400 Bad Request, 404 Not Found, 500 Internal Server Error, 503 Service Unavailable.

## Data contracts (summary)
Domain vs wire types:
- Domain types from `commons/oprc-models`: `OPackage`, `OClassDeployment`, function-level overrides (including `ProvisionConfig`).
- Wire/transport types from `commons/oprc-grpc` (protobuf): `DeploymentUnit`, `FunctionDeploymentSpec`, `ProvisionConfig`, `NfrRequirements`, etc.

Key structures:
- `OPackage` — package metadata, functions, classes, and dependencies.
- `OClassDeployment` — target package/class, `target_env`, `target_clusters: string[]`, deployment-level NFR requirements, optional ODGM hints, and per-function overrides.
- `DeploymentUnit` (protobuf) — per-cluster unit created from an `OClassDeployment` that PM sends to CRM.
  - Per-function `provision_config` is the single source for container image and autoscaling bounds (`min_scale`/`max_scale`).
  - Per-function `nfr_requirements` are included; PM currently defaults them from the deployment-level NFRs when not provided at function level.
  - ODGM hints from `OClassDeployment.odgm` are mapped to protobuf `odgm_config` for CRM to render collections/routes. `replica_count` is set from PM's computed target replicas.

Notes:
- The `create_deployment` handler currently fabricates a placeholder `OClass` description; package→class resolution is a planned improvement.
- `ClassRuntimeStatus` now uses enums (condition from `oprc-models::DeploymentCondition`, phase as a PM enum). JSON uses SCREAMING_SNAKE_CASE for enum values.
- Replicas are not a field on function specs; CRM derives Kubernetes `spec.replicas` from `provision_config.min_scale` when rendering templates.

## Configuration (env)
Env-first via `envconfig` with sensible defaults. Common ones:

Server
- `SERVER_HOST` (default `0.0.0.0`)
- `SERVER_PORT` (default `8080`)
- `SERVER_WORKERS` (optional)

Storage
- `STORAGE_TYPE` = `memory` | `etcd` (default `memory`)
- Etcd (planned; not yet implemented in factory):
  - `ETCD_ENDPOINTS` (comma-separated; default `localhost:2379`)
  - `ETCD_KEY_PREFIX` (default `/oaas/pm`)
  - `ETCD_USERNAME`, `ETCD_PASSWORD`
  - `ETCD_TIMEOUT` (seconds; default `30`)
  - `ETCD_TLS_ENABLED` (bool; default `false`)
  - `ETCD_CA_CERT_PATH`, `ETCD_CLIENT_CERT_PATH`, `ETCD_CLIENT_KEY_PATH`, `ETCD_TLS_INSECURE`

CRM
- `CRM_DEFAULT_URL` (e.g., `http://localhost:8088`) — enables a `default` cluster. This is the tonic endpoint (http/https) used for gRPC.
- `CRM_DEFAULT_TIMEOUT` (seconds; default `30`)
- `CRM_DEFAULT_RETRY_ATTEMPTS` (default `3`)
- `CRM_HEALTH_CHECK_INTERVAL` (seconds; default `60`)
- `CRM_CIRCUIT_BREAKER_FAILURE_THRESHOLD` (default `5`) and
  `CRM_CIRCUIT_BREAKER_TIMEOUT` (seconds; default `60`)
 - `CRM_HEALTH_CACHE_TTL` (seconds; default `15`) — health result cache TTL per cluster.

Deployment policy
- `DEPLOY_MAX_RETRIES` (default `2`) — number of retry attempts per cluster (total attempts = 1 + retries) during deploy.
- `DEPLOY_ROLLBACK_ON_PARTIAL` (default `false`) — if true, any partial success triggers rollback (delete) of successful clusters and overall failure.
- `PACKAGE_DELETE_CASCADE` (default `false`) — if true, package delete attempts to delete all its deployments first.

Observability
- `RUST_LOG` controls log level (e.g., `debug`, `info`). The binary configures JSON logs by default via `tracing_subscriber`.
- Additional app config switches exist but are not fully wired yet:
  - `OBSERVABILITY_ENABLED` (default `true`)
  - `LOG_LEVEL` (default `info`), `LOG_FORMAT` (default `json`)
  - `METRICS_ENABLED` (default `true`), `METRICS_PORT` (default `9090`)
  - `TRACING_ENABLED` (default `false`), `TRACING_ENDPOINT`

## Quick start
Build and run
- Build: `cargo build -p oprc-pm`
- Run: set `RUST_LOG=info` and optionally `CRM_DEFAULT_URL` (gRPC endpoint), then `cargo run -p oprc-pm`
- Health: `GET http://localhost:8080/health`

Minimal flow
1. Create one or more packages (POST `/api/v1/packages`).
2. Create a class deployment (POST `/api/v1/deployments`). Ensure `target_clusters` contain cluster names that PM knows (by default, `default`).
3. Check status via `/api/v1/deployment-status/{id}` (proxied via gRPC) or browse records via `/api/v1/deployment-records`.

Docker Compose
- See `docker-compose.yml` in this folder for a local composition scaffold.

## Multi-cluster behavior
PM manages a map of cluster → CRM client. Current implemented features:
- Default cluster via `CRM_DEFAULT_URL`.
- Per-cluster deployment ID mapping persisted (logical deployment key → cluster deployment unit ID) supporting scoped delete/status.
- Aggregated health (`/clusters/health`) + per-cluster health.
- Fan-out listing of deployment records across clusters.
 - Health caching (TTL via `CRM_HEALTH_CACHE_TTL`) reduces duplicate gRPC calls.
 - Cluster selection orders healthy clusters first; degraded/unreachable appended after healthy ones.
 - Configurable deploy retry & optional rollback on partial failure (see deployment policy env vars).
 - Optional cascade delete (`PACKAGE_DELETE_CASCADE`) of deployments on package removal.
 - Basic auto-deploy: embedded `package.deployments` specs are scheduled if not already deployed.
 - Availability signals: each cluster health response may include `availability` (0..1). PM consumes this for replica planning; missing values cause a deployment validation error rather than a silent fallback.

### Deployment retry semantics
For each target cluster the PM will attempt an initial deploy plus up to `DEPLOY_MAX_RETRIES` retry attempts (total attempts = 1 + `DEPLOY_MAX_RETRIES`). A retry is triggered only when the gRPC `Deploy` call returns a non-OK status or the CRM client cannot be acquired. Backoff is a simple linear 200ms * attempt number. Successful attempts persist a per‑cluster deployment unit id mapping. If all clusters fail, the deployment returns HTTP 400 (`Deployment failed in all clusters`). If some clusters succeed:

* When `DEPLOY_ROLLBACK_ON_PARTIAL=true`, PM issues delete calls for successful clusters and reports failure.
* When false (default), PM preserves successful mappings and returns the first successful per‑cluster id.

### Per‑cluster deletion semantics
`DELETE /api/v1/deployments/{key}?cluster={name}` requires the logical deployment key and the cluster query parameter; the PM resolves the stored (logical key → cluster unit id) mapping and forwards the cluster‑specific id to CRM. Supplying a cluster without a stored mapping is a no‑op (404 if the logical deployment is missing).

### Health caching
Cluster health results are cached for `CRM_HEALTH_CACHE_TTL` seconds (default 15) to avoid fan‑out storms under repeated `/clusters/health` access. The cache is per cluster and only stores SERVING / NOT_SERVING status and timestamp. Set TTL to `0` to disable caching.

### Cascade delete
When `PACKAGE_DELETE_CASCADE=true`, package deletion enumerates all active deployments owned by the package and issues cluster‑scoped deletes prior to removing the package metadata. Failures are logged; best-effort cleanup proceeds.

### Summary of related env knobs
| Variable | Effect |
|----------|--------|
| `DEPLOY_MAX_RETRIES` | Per-cluster retry count (not including the first attempt). |
| `DEPLOY_ROLLBACK_ON_PARTIAL` | Roll back successful clusters when any cluster fails. |
| `PACKAGE_DELETE_CASCADE` | Delete deployments before removing a package. |
| `CRM_HEALTH_CACHE_TTL` | Seconds to cache health responses. |

### Availability‑driven replica sizing

PM derives target replicas for each deployment using per‑cluster availability signals and (optionally) the class state consistency model:

1. Target availability (`nfr_requirements.availability`) defaults to `0.99` if unspecified.
2. For each target cluster, PM obtains `availability` from CRM (`CrmInfoService`). If absent, it derives a readiness ratio (`ready_nodes / node_count`). If neither is available, the deployment is rejected.
3. The per‑cluster availabilities are sorted worst → best. PM incrementally adds replicas (worst first) and computes the probability that a majority (quorum) of the selected replicas are simultaneously up (dynamic programming over Bernoulli node up/down states). It stops at the minimal replica count whose quorum availability meets or exceeds the target. Hard cap: 50.
4. Strong consistency: if the class `state_spec.consistency_model == Strong`, PM enforces a minimum of `2f+1` replicas (where `f` is the inferred tolerable failures given the provisional replica count). This may increase the replica count selected by availability logic.
5. The achieved quorum availability and the selected replica count are logged (future: surfaced via status endpoint).
6. Application of results: PM writes the selected replica count into each function's `provision_config.min_scale` in the protobuf `DeploymentUnit`, and sets `odgm_config.replica_count` accordingly. CRM templates render Kubernetes `Deployment.spec.replicas` from `min_scale`.

Legacy independent availability formula (`A=1-(1-p)^R`) is retained only for test reference; production logic uses quorum DP.

Edge cases & safeguards:
* Empty availability list → error.
* Reported node_count == 0 → error.
* Values are clamped to [0,1].
* Strong consistency imposes a minimum fault tolerance of one (maps to 3 replicas when base result is 1).

Planned enhancements:
* Configurable replica cap (currently 50) and algorithm selection (independent vs quorum).
* Exposure of achieved availability + inputs via an inspection API.
* Hysteresis to avoid thrash during rapid availability changes.

## Additional tests (multi-cluster policy)
The file `tests/features_multicluster_policy_tests.rs` contains focused policy & behavior tests:
* `health_caching_reduces_grpc_calls` — validates health result caching.
* `deploy_retries_succeed_without_rollback` — exercises retry path without rollback.
* `package_delete_cascade_removes_deployments` — verifies cascade removal behavior.
* `availability_field_flows_end_to_end` — validates that a fixed availability surfaced by CRM propagates through PM's `/clusters` endpoint (file: `tests/availability_flow_tests.rs`).

Run a single test (PowerShell):
```
cargo test -p oprc-pm --test features_multicluster_policy_tests -- --exact deploy_retries_succeed_without_rollback --nocapture
```

These supplement the broader integration tests in `it_pm_crm.rs`.

Selection uses `target_clusters` in the `OClassDeployment`. Partial failures during multi-cluster deploy currently log warnings and retain successful cluster mappings (rollback of failed clusters is deferred and may become configurable).

## Implementation notes
- Logging: `tracing_subscriber` with `EnvFilter` (honors `RUST_LOG`) and JSON output.
- Server composition can be built for tests via `bootstrap::build_api_server_from_env()`.
- Storage factory in `storage/factory.rs` returns `MemoryStorageFactory`; etcd path is a TODO.
- CRM client is gRPC-only (tonic). Health checks use the gRPC Health service; Deploy/Status/Delete/List/Get record use `commons/oprc-grpc` clients.

## Testing
Unit / integration / e2e tasks are consolidated in `control-plane/justfile`.

Common commands (PowerShell):

```
just -f control-plane/justfile unit     # All CRM + PM unit tests
just -f control-plane/justfile pm-it    # PM integration tests
just -f control-plane/justfile crm-it   # CRM ignored operator tests
just -f control-plane/justfile e2e      # PM↔CRM end-to-end (ignored)
```

Direct cargo examples:
- Run all PM tests: `cargo test -p oprc-pm`.
- PM↔CRM integration (file): `cargo test -p oprc-pm --test it_pm_crm -- --nocapture`.
  - Notable cases:
    - `cluster_health_with_mock` — mocks gRPC Health to validate `/clusters/health` aggregation.
    - `list_deployment_records_with_mock` — mocks DeploymentService to return items.
    - `e2e_with_kind_crm_happy_path` (ignored) — exercises embedded CRM + Kubernetes reconciliation.

## Integration test checklist (PM ↔ CRM + echo)

- [ ] Ensure `.env` defines an image prefix (used by docker compose): `IMAGE_PREFIX=...` (using `IMAGE_PREFIX=ghcr.io/pawissanutt/oaas-rs`, `IMAGE_VERSION=latest`).
- [ ] Use the prebuilt echo function image: `${IMAGE_PREFIX}/echo-fn:latest` (no build step required).
- [ ] Fast lane (no k8s): run PM integration tests that use in-process server and memory storage.
  - Implemented as `tests/it_pm_crm.rs::inproc_deploy_smoke`.
  - Verifies package CRUD and deployment creation; checks `/health`.
  - Run: `cargo test -p oprc-pm --test it_pm_crm -- --exact inproc_deploy_smoke --nocapture`
- [ ] Mocked lane (no k8s): start a local tonic gRPC Health server and point `CRM_DEFAULT_URL` to it to assert PM’s CRM health read.
  - Implemented as `tests/it_pm_crm.rs::cluster_health_with_mock`.
  - Verifies `GET /api/v1/clusters/health` (and per-cluster) reflects SERVING/NOT_SERVING from the mock Health service.
  - Run: `cargo test -p oprc-pm --test it_pm_crm -- --exact cluster_health_with_mock --nocapture`
- [ ] k8s lane (ignored by default):
  - [ ] From `control-plane/oprc-crm`, bring up an ephemeral cluster and CRDs: `just it-kind-up`.
  - [ ] If needed for your cluster, load the echo image into kind (so the node can pull `${IMAGE_PREFIX}/echo-fn:latest`).
  - [ ] Start the in-process CRM controller and gRPC in the PM test (handled by `e2e_with_kind_crm_happy_path`).
  - [ ] Run the ignored PM→CRM E2E: `cargo test -p oprc-pm --test it_pm_crm -- --exact e2e_with_kind_crm_happy_path --ignored --nocapture`.
  - [ ] Tear down the cluster: `just it-kind-down`.

Notes
- PM loads configuration from environment variables; YAML files under `config/` are examples. Set `CRM_DEFAULT_URL` for tests.
- Health is standardized: CRM exposes both HTTP `/health` and the gRPC Health service. PM’s `CrmClient` uses the gRPC Health service.

## Main checklist (PM roadmap)

M1 — Core API and in-process tests (baseline)
- [x] Package CRUD and memory storage
- [x] Deployment API with package/class resolution and auto function specs
- [x] Unified health endpoint at `/health`
- [x] In-process + mocked CRM tests (`tests/it_pm_crm.rs`)

M2 — Real CRM integration (gRPC)
- [x] Replace placeholder CRM client with gRPC using `commons/oprc-grpc` (tonic)
- [x] Implement basic Deploy/Status/Delete via gRPC
- [x] Align status mapping for ClassRuntime (list uses summarized enum; no N+1)
- [x] Surface resource refs in responses
- [x] Add ignored k8s E2E suite with kind that exercises PM↔CRM happy path (test `e2e_with_kind_crm_happy_path` + just `e2e` target)

M3 — Multi‑cluster and lifecycle
- [x] Track per‑cluster deployment IDs, support scoped delete/update
 - [x] Improve cluster selection (health-aware) and caching in `CrmManager`
 - [x] Add rollback/retry policy for partial failures across clusters
 - [x] Auto-deploy scheduling from package metadata (`DeploymentService::schedule_deployments`) — basic implementation
 - [x] On package delete, optionally cascade/validate active deployments

M4 — NFR‑aware planning
- [ ] Compute requirements from `nfr_requirements` and function metadata
- [ ] Merge per-function overrides; surface effective spec in DeploymentUnit
- [ ] Prepare for enforcement feedback (consume CRM recommendations when available)

M5 — Validation, errors, and storage (after core flows)
- [ ] Strengthen request validation (schema + business rules) and map errors to 400/404 vs 500
- [ ] Implement etcd storage backend in `storage/factory.rs` (TLS-ready) and document setup
- [ ] Add pagination, ordering, and richer filters to list endpoints (packages/deployments); add storage indexes for common filters
- [ ] Add negative/idempotency tests and basic race/concurrency tests

M6 — Observability, security, docs
- [ ] Prometheus metrics for API and storage; OpenTelemetry tracing wiring
- [ ] Secure CRM connectivity (TLS/mTLS, API keys); secret handling guidance
- [ ] Samples and docs: echo package/deployment, Docker Compose walkthrough
- [ ] Document request/response schemas and the error model

## References
- Shared types: `commons/oprc-models`
- gRPC contracts: `commons/oprc-grpc`
- CRM contract and flows: `control-plane/oprc-crm/README.md`
- NFR Enforcement Design: `docs/NFR_ENFORCEMENT_DESIGN.md`
- Storage traits: `commons/oprc-cp-storage`
