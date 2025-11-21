# OaaS‑RS Copilot Instructions

Goal: Give AI agents the minimum, concrete context to be productive in this Rust OaaS (Object‑as‑a‑Service) monorepo.

## Big picture
- **Data Plane**: `oprc-odgm/` (stateful object grid), `oprc-gateway/` (REST/gRPC ingress), `oprc-router/` (Zenoh + ZRPC routing).
- **Control Plane**: `oprc-crm/` (Kubernetes controller + gRPC), `oprc-pm/` (Package Manager REST → CRM gRPC).
- **Tools & Tests**: `tools/oprc-cli` (Main CLI), `tests/system_e2e` (Full system validation via Kind).

## Core idioms (copy/paste ready)
- **Zenoh config**: `let cfg = oprc_zenoh::OprcZenohConfig::init_from_env()?; let session = zenoh::open(cfg.create_zenoh()).await?;`
- **ZRPC routing**: `let r = Routable { cls, func, partition }; let mut conn = conn_manager.get_conn(r).await?;`
- **gRPC contracts**: `commons/oprc-grpc` (DeploymentService, Health, CrmInfoService).

## Config & logging conventions
- **Env‑first**: `#[derive(Envconfig)]` struct C { #[envconfig(from="HTTP_PORT", default="8080")] port: u16 }
- **Common envs**: `RUST_LOG`, `OPRC_ZENOH_PEERS`, `ODGM_*`, `OPRC_PM_*`.
- **Tracing**: `tracing::info!`, `tracing::debug!`. Logs controlled via `RUST_LOG`.

## Critical Workflows
- **Full System Test**: `just system-e2e` (Builds images, deploys to Kind, runs scenarios). **Primary validation method.**
- **Build**: `cargo build -r` (workspace) or `just build release`.
- **Run CRM (Dev)**: `RUST_LOG=debug HTTP_PORT=8088 cargo run -p oprc-crm`.
- **Deploy K8s**: `just deploy` (uses `k8s/charts/deploy.sh`).
- **Install CLI**: `just install-tools` (installs `oprc-cli`).

## Tests (where and how)
- **System E2E**: `tests/system_e2e`. Uses `oprc-cli` to validate end-to-end flows on Kind.
    - Debug: `RUST_LOG=debug,system_e2e=debug just system-e2e`.
    - Clean: `just system-e2e-clean`.
- **Unit/Integration**: `just -f control-plane/justfile unit | crm-it | pm-it`.
- **Async Tests**: `#[tokio::test(flavor = "multi_thread")]`.
- **Logging in Tests**: Use `#[test_log::test(tokio::test)]` to capture logs.

## File/map you’ll reference first
- **CLI Commands**: `tools/oprc-cli/src/commands/` & `lib.rs`.
- **E2E Scenarios**: `tests/system_e2e/src/main.rs` & `cli.rs`.
- **ODGM wiring**: `data-plane/oprc-odgm/src/lib.rs`.
- **Gateway routing**: `data-plane/oprc-gateway/src/handler`.
- **CRM flows**: `control-plane/oprc-crm/README.md`.

## Integration patterns to mirror
- **CLI → System**: CLI uses `oprc-pm` (REST) for config and `oprc-gateway` (HTTP/gRPC) for invocation.
- **ODGM Integration**: Function pods receive `ODGM_ENABLED`, `ODGM_SERVICE` from CRM.
- **PM → CRM**: PM fans out to CRM via gRPC clients (`commons/oprc-grpc`).

## Code style & conventions
- **Error Handling**: `anyhow::Result` for binaries (like CLI/E2E), `thiserror` for libraries.
- **Async**: `tokio` runtime.
- **Style**: Rust 2024, `rustfmt`.
- **Dependencies**: Centralized in root `Cargo.toml`.
