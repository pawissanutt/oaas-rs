# OaaS Package Manager (PM)

Service that manages OaaS packages and orchestrates class/function deployments across one or more CRM-managed clusters. PM exposes a REST API and talks to each CRM via gRPC (tonic). Storage is pluggable (memory now, etcd planned).

## What it does
- Package registry CRUD (stores `OPackage` from `commons/oprc-models`).
- Deployment orchestration for classes (`OClassDeployment` → per-cluster `DeploymentUnit`).
- Multi-cluster awareness via a simple CRM manager; default cluster support out of the box.
- Read-through views for Deployment Records and Status via CRM.

Current constraints
- PM→CRM uses gRPC and the Health service; Deploy/Status/Delete are implemented, but some mappings are minimal.
- List/Get DeploymentRecords RPCs exist; CRM currently returns an empty list by default until CRD querying is wired.
- Idempotency/correlation IDs and advanced timeouts/retries are basic and will be hardened.
- Storage backends: in-memory is implemented; etcd is stubbed.

## Architecture at a glance
- HTTP API (Axum) on a single port.
- Services:
  - PackageService — validates and stores packages and optionally schedules deployments.
  - DeploymentService — creates `DeploymentUnit`s and calls CRM per target cluster; persists `OClassDeployment`.
- CRM Manager — resolves a per-cluster client; supports default cluster selection and health reads.
- Storage — via `commons/oprc-cp-storage` traits; memory backend in use.

Key modules
- `src/server.rs` — Axum routes and server wiring.
- `src/services/{package,deployment}.rs` — business logic.
- `src/crm/{manager,client}.rs` — CRM fanout and gRPC client (tonic) using `commons/oprc-grpc`.
- `src/config/app.rs` — env-first config with helpers for server/storage/CRM/observability.

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

Deployment records and status (proxied to CRM)
- GET `/deployment-records` → Aggregate across clusters (optional filters: `package_name`, `class_key`, `environment`, `cluster`, `status`, `limit`, `offset`).
- GET `/deployment-records/{id}` → Fetch by id (searches specified cluster or all clusters).
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
Types come from `commons/oprc-models`.
- `OPackage` — package metadata, functions, classes, and dependencies.
- `OClassDeployment` — target package/class, `target_env`, `target_clusters: string[]`, per-function overrides, NFR requirements.
- `DeploymentUnit` — per-cluster unit created from an `OClassDeployment` that PM sends to CRM.

Note: The `create_deployment` handler currently fabricates a placeholder `OClass` description; package→class resolution is a planned improvement.

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
- PM can hold multiple CRM clients; today it constructs a single `default` client from `CRM_DEFAULT_URL`.
- The CRM manager exposes:
  - enumeration (`GET /api/v1/clusters`),
  - aggregated health across clusters (`GET /api/v1/clusters/health`) and per-cluster health,
  - fan-out listing of deployment records.
- Deployment selection to clusters uses the `target_clusters` in the `OClassDeployment`.

## Implementation notes
- Logging: `tracing_subscriber` with `EnvFilter` (honors `RUST_LOG`) and JSON output.
- Server composition can be built for tests via `bootstrap::build_api_server_from_env()`.
- Storage factory in `storage/factory.rs` returns `MemoryStorageFactory`; etcd path is a TODO.
- CRM client is gRPC-only (tonic). Health checks use the gRPC Health service; Deploy/Status/Delete/List/Get record use `commons/oprc-grpc` clients.

## Testing
- Unit/integration test scaffolding is present under `src/tests/` in this crate and across the workspace.
- Run all tests: `cargo test -p oprc-pm`.
- Run new PM↔CRM integration tests only: `cargo test -p oprc-pm --test it_pm_crm -- --nocapture`.

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
  - [ ] Start PM tests configured with `CRM_DEFAULT_URL` targeting the CRM endpoint used in tests.
  - [ ] Run the ignored PM→CRM tests; then tear down the cluster with `just it-kind-down`.

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
- [ ] Align response mapping to DeploymentRecord status and resource refs
- [ ] Add ignored k8s E2E suite with kind that exercises PM↔CRM happy path

M3 — Multi‑cluster and lifecycle
- [ ] Track per‑cluster deployment IDs, support scoped delete/update
- [ ] Improve cluster selection (health-aware) and caching in `CrmManager`
- [ ] Add rollback/retry policy for partial failures across clusters
- [ ] Auto-deploy scheduling from package metadata (`DeploymentService::schedule_deployments`)
- [ ] On package delete, optionally cascade/validate active deployments

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
- Storage traits: `commons/oprc-cp-storage`
