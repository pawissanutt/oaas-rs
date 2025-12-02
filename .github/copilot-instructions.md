# OaaS‑RS Copilot Instructions

Goal: Give AI agents the minimum, concrete context to be productive in this Rust OaaS (Object‑as‑a‑Service) monorepo.

## Big picture
OaaS-RS is a Rust reimplementation of [Oparaca](https://github.com/hpcclab/OaaS) — an Object-as-a-Service platform.

- **Control Plane**: `control-plane/oprc-crm/` (Kubernetes controller managing `ClassRuntime` CRDs + gRPC API), `control-plane/oprc-pm/` (Package Manager REST API → fans out to CRM(s) via gRPC).
- **Data Plane**: `data-plane/oprc-odgm/` (Object Data Grid with Raft consensus), `data-plane/oprc-gateway/` (stateless REST/gRPC → Zenoh translation), `data-plane/oprc-router/` (Zenoh pub/sub + ZRPC routing).
- **Commons**: `commons/` — shared libs: `oprc-grpc` (protobuf contracts), `oprc-zenoh` (Zenoh config), `oprc-zrpc` (RPC over Zenoh), `oprc-models`, storage abstractions.
- **Tools & Tests**: `tools/oprc-cli` (main CLI), `tests/system_e2e` (full Kind cluster validation), `frontend/oprc-gui` (Dioxus 0.7 GUI).

### Data flow
1. **Deploy**: Client → PM (REST) → CRM (gRPC) → upsert `ClassRuntime` CRD → reconcile → apply K8s resources.
2. **Invoke**: Client → Gateway (REST/gRPC) → Router (Zenoh) → Function runtime or ODGM.

## Core idioms (copy/paste ready)
```rust
// Zenoh session init
let cfg = oprc_zenoh::OprcZenohConfig::init_from_env()?;
let session = zenoh::open(cfg.create_zenoh()).await?;

// Environment config (all services use this pattern)
#[derive(Envconfig, Clone, Debug)]
pub struct MyConfig {
    #[envconfig(from = "HTTP_PORT", default = "8080")]
    pub port: u16,
}
```

- **gRPC contracts**: `commons/oprc-grpc/proto/` — `DeploymentService`, `Health`, `CrmInfoService`, `DataService`.
- **Storage traits**: `commons/oprc-cp-storage/src/traits.rs` (PackageStorage, DeploymentStorage).

## Config & logging conventions
- **Env‑first**: All configs derive `Envconfig`. Key envs: `RUST_LOG`, `OPRC_ZENOH_PEERS`, `ODGM_*`, `OPRC_PM_*`, `CRM_*`.
- **Tracing**: `tracing::{info,debug,error}!` macros. Control via `RUST_LOG=debug,oprc_crm=trace`.
- **OpenTelemetry**: Optional OTLP export via `commons/oprc-observability`. Deploy stack: `./k8s/charts/deploy-observability.sh install`.

## Critical Workflows
| Task | Command |
|------|---------|
| **Build all** | `cargo build -r` or `just build release` |
| **Check code compiles** | `cargo check --workspace` |
| **Full E2E (primary validation)** | `just system-e2e` |
| **Clean E2E cluster** | `just system-e2e-clean` |
| **Unit tests** | `just -f control-plane/justfile unit` |
| **Integration tests** | `just -f control-plane/justfile crm-it` / `pm-it` / `all-it` |
| **Run CRM locally** | `RUST_LOG=debug HTTP_PORT=8088 cargo run -p oprc-crm` |
| **Deploy to K8s** | `just deploy` (uses `k8s/charts/deploy.sh`) |
| **Install CLI** | `just install-tools` |
| **Create Kind cluster** | `just create-cluster oaas-e2e` |

## Tests (where and how)
- **System E2E**: `tests/system_e2e/` — uses `oprc-cli` against Kind cluster. Debug: `RUST_LOG=debug,system_e2e=debug just system-e2e`.
- **Async tests**: Use `#[tokio::test]` or `#[tokio::test(flavor = "multi_thread")]`.
- **Logging in tests**: `#[test_log::test(tokio::test)]` to capture tracing output.
- **Integration tests**: Marked `#[ignore]`, run via `cargo test -p oprc-crm --test it_controller -- --ignored`.

## File map (start here)
| Area | Key files |
|------|-----------|
| CLI commands | `tools/oprc-cli/src/commands/*.rs` |
| E2E scenarios | `tests/system_e2e/src/{main,cli}.rs` |
| ODGM core | `data-plane/oprc-odgm/src/lib.rs`, `cluster.rs`, `shard/` |
| Gateway handlers | `data-plane/oprc-gateway/src/handler/` |
| CRM controller | `control-plane/oprc-crm/src/controller/` |
| PM services | `control-plane/oprc-pm/src/services/` |
| gRPC protos | `commons/oprc-grpc/proto/*.proto` |

## Code style & conventions
- **Error handling**: `anyhow::Result` for binaries, `thiserror` for library error enums.
- **Async runtime**: `tokio` (full features).
- **Edition**: Rust 2024, `rust-version = "1.85"`.
- **Dependencies**: Centralized in root `Cargo.toml` `[workspace.dependencies]`.
- **Formatting**: `rustfmt` (config in `rustfmt.toml`).
- **Validation**: Always run `cargo check --workspace` after edits to verify code compiles.
- **Terminal commands**: Run commands directly (e.g., `cargo check`), NOT via `bash -c "cargo check"` — wrapping in bash or using redirections like `2>&1` or `2>/dev/null` blocks auto-approval.
- **Context length**: For commands with potentially long output, use flags to reduce verbosity (e.g., `cargo check -q`, `cargo test -q`) to avoid exceeding context limits.

## Frontend (Dioxus GUI)
Located at `frontend/oprc-gui/`. See `AGENTS.md` for Dioxus 0.7 patterns.
- `use_signal` for local state, `use_context_provider`/`use_context` for global.
- **Check builds**: `cargo check -p oprc-gui` (developer runs `dx serve` separately).
