# OaaS‑RS Copilot Instructions

Goal: Give AI agents the minimum, concrete context to be productive in this Rust OaaS (Object‑as‑a‑Service) monorepo.

## Big picture
- Data plane: `oprc-odgm/` (stateful object grid), `oprc-gateway/` (REST/gRPC ingress), `oprc-router/` (Zenoh + ZRPC routing).
- Control plane: `oprc-crm/` (Kubernetes controller + gRPC), `oprc-pm/` (Package Manager REST → CRM gRPC).
- Shared crates: `commons/` → `oprc-grpc` (protos/clients), `oprc-zenoh` (Zenoh cfg), `oprc-models`, `oprc-*/storage`.

## Core idioms (copy/paste ready)
- Zenoh config/session (commons/oprc-zenoh):
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env()?; let session = zenoh::open(cfg.create_zenoh()).await?;
- ZRPC routing pattern: let r = Routable { cls, func, partition }; let mut conn = conn_manager.get_conn(r).await?;
- gRPC contracts (commons/oprc-grpc): deployment.DeploymentService { Deploy, GetDeploymentStatus, DeleteDeployment }, grpc.health.v1.Health, plus CrmInfoService (cluster availability).

## Config & logging conventions
- Env‑first via `envconfig` (derive): #[derive(Envconfig)] struct C { #[envconfig(from="HTTP_PORT", default="8080")] port: u16 }
- Common envs: RUST_LOG, OPRC_ZENOH_PEERS/PORT/MODE, ODGM_*, OPRC_PM_*, HTTP_PORT/GRPC_PORT.
- CRM feature flags you’ll see in code/tests: OPRC_CRM_FEATURES_ODGM, OPRC_CRM_FEATURES_NFR_ENFORCEMENT, OPRC_CRM_FEATURES_HPA.
- Tracing: registry + fmt + EnvFilter(RUST_LOG); use tracing::* throughout.

## Daily workflows
- Build: cargo build -r (workspace). 
- Run CRM (dev): RUST_LOG=debug HTTP_PORT=8088 cargo run -p oprc-crm.
- Generate CRD: cargo run -p oprc-crm --bin crdgen > k8s/crds/deploymentrecords.gen.yaml.
- Docker Compose: just compose-dev | just build-release | just compose-release (see root justfile).

## Tests (where and how)
- Control‑plane consolidated: just -f control-plane/justfile unit | crm-it | pm-it | all-it.
- Notable CRM enforcement fallback (no HPA): cargo test -p oprc-crm --test it_enforce -- --ignored --exact enforce_fallback_updates_deployment_when_hpa_absent.
- Async tests use #[tokio::test(flavor = "multi_thread")]; logs via RUST_LOG.

## File/map you’ll reference first
- ODGM server wiring: data-plane/oprc-odgm/src/lib.rs
- Gateway routing: data-plane/oprc-gateway/src/handler
- Zenoh helpers: commons/oprc-zenoh/src/lib.rs
- gRPC protos/clients: commons/oprc-grpc/proto and src
- CRM behavior & flows: control-plane/oprc-crm/README.md (templates, ODGM env injection, NFR enforcement)

## Integration patterns to mirror
- ODGM integration: function pods get ODGM_ENABLED, ODGM_SERVICE, ODGM_COLLECTION (JSON) from CRM templates.
- PM → CRM: PM REST calls fan out to CRM via tonic clients from `commons/oprc-grpc`; health via gRPC Health and CrmInfoService.

## Storage abstraction (DP)
- Traits and types under `commons/oprc-dp-storage`; default feature is `memory`. Optional backends: `redb`, `fjall`, `rocksdb` via cargo features.

## Error handling patterns
- Use `anyhow::Result` for application errors in binaries
- Use `thiserror` for library errors with custom error types
- Propagate errors with `?` operator; avoid unwrap/expect in production code unless justified
- gRPC services return `tonic::Status` for error responses

## Code style & conventions
- Follow Rust 2021 edition idioms
- Use `rustfmt` (configured via `rustfmt.toml` at root)
- Async runtime: `tokio` with `#[tokio::main]` or `#[tokio::test]`
- Prefer `tracing::` macros over `log::` for structured logging
- Use `#[inline]` for small, performance-sensitive functions
- Keep functions focused and modules small; refactor large files into submodules

## Dependencies & updates
- Workspace dependencies are centralized in root `Cargo.toml` `[workspace.dependencies]`
- Before adding new dependencies, check if a suitable crate exists in workspace dependencies
- Pin versions for critical dependencies; use `^` for minor updates on others
- Feature flags: use workspace features for optional backends (e.g., `redb`, `fjall`, `rocksdb` for storage)

## Tips
- Prefer shared crates/types over redefining. Follow existing envconfig + tracing + health/gRPC scaffolding when adding a service
- Keep configs env‑first and reuse patterns from the files listed above
- Avoid large files; break them into smaller, focused modules. Apply the DRY principle
- When adding a new service, use existing services as templates (e.g., oprc-gateway structure for REST/gRPC services)
