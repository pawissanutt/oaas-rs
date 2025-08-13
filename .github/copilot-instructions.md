# OaaS-RS Copilot Instructions

Purpose: Give AI agents the minimum, concrete context to be productive in this Rust OaaS (Object-as-a-Service) workspace.

## Architecture at a glance
- Data plane
    - ODGM — `data-plane/oprc-odgm/`: stateful object grid (replaces Java Invoker); sharded, low-latency.
    - Gateway — `data-plane/oprc-gateway/`: external REST/gRPC ingress.
    - Router — `data-plane/oprc-router/`: Zenoh-based messaging and ZRPC routing.
- Control plane
    - CRM — `control-plane/oprc-crm/`: Kubernetes controller that reconciles a DeploymentRecord CRD; gRPC API to PM.
    - PM — `control-plane/oprc-pm/`: Package Manager, talks to CRM via gRPC.
- Commons — `commons/`: shared crates: `oprc-grpc` (protobufs), `oprc-zenoh` (Zenoh cfg), `oprc-models`, storage libs.

## Communication patterns (use these idioms)
- Zenoh session and config
    - let cfg = oprc_zenoh::OprcZenohConfig::init_from_env()?; let session = zenoh::open(cfg.create_zenoh()).await?;
- ZRPC routing
    - let r = Routable { cls, func, partition }; let mut conn = conn_manager.get_conn(r).await?;
- gRPC contracts (CRM)
    - Services in `commons/oprc-grpc`: deployment.DeploymentService { Deploy, GetDeploymentStatus, DeleteDeployment } and health.

## Configuration and logging (project-wide conventions)
- Env-first config via `envconfig`:
    - #[derive(Envconfig)] struct MyCfg { #[envconfig(from = "HTTP_PORT", default = "8080")] port: u16 }
    - Common envs: SERVICE_LOG, OPRC_ZENOH_PEERS, ODGM_*, OPRC_PM_*, GRPC_PORT.
- Tracing setup (keep consistent): registry + fmt + EnvFilter(SERVICE_LOG). Use tracing::* macros liberally.

## Workspace and dependencies
- Single Cargo workspace; add deps in root `Cargo.toml` under [workspace.dependencies], then reference in crates with `{ workspace = true }`.
- Shared types live in `commons/*` — prefer reusing them over re-defining models or errors.

## Build, run, and tools
- Build binaries: cargo build -r
- Compose (release images): docker compose -f docker-compose.release.yml build | up -d
- just helpers (see `justfile`):
    - just compose-dev (docker compose up -d)
    - just build-release; just compose-release
    - just install-tools (installs `oprc-cli`, util tools)
- CLI: tools/oprc-cli (install with cargo install --path tools/oprc-cli). Useful for local object ops and invocations.

## Testing patterns
- Async tests with #[tokio::test(flavor = "multi_thread")]; enable logs via test-log.
- Integration style: serial_test for ordered flows; set env like STORAGE_TYPE=memory in setup.

## Files to look at first
- data-plane/oprc-odgm/src/lib.rs — ODGM server wiring and collections.
- data-plane/oprc-gateway/src/handler — HTTP/gRPC routing.
- commons/oprc-zenoh/src/lib.rs — Zenoh config helpers.
- commons/oprc-grpc/proto — Protos for deployment, runtime, package, health.
- control-plane/oprc-crm/README.md — PM↔CRM flows, CRD mapping, deadlines, idempotency.

## Practical workflows (copy these)
- Generate CRD YAML (Windows PowerShell): cargo run -p oprc-crm --bin crdgen | Out-File -FilePath k8s/crds/deploymentrecords.gen.yaml -Encoding utf8
- Quick smoke: cargo run -p data-plane/oprc-dev --bin check-delay (after just install-tools)
- Sanity via CLI: see just check-status (invokes ops across partitions via zenoh).

## Storage abstraction
- Implement `ApplicationDataStorage` (see `commons/oprc-dp-storage/src/traits`); use `StorageValue` types from the same crate.

Tips
- Prefer small modules and shared helpers over ad-hoc code. Follow existing Zenoh/gRPC patterns. Add tracing lines for major branches.
- If adding a service, mirror envconfig + tracing + health/grpc setup found in existing services.
```
