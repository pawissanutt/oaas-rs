# OaaS-RS Copilot Instructions

## Project Overview

This is a Rust reimplementation of the Oparaca Object-as-a-Service platform, focusing on the data plane while maintaining Java control plane compatibility. The system replaces the original Java "Invoker" with ODGM (Object Data Grid Manager) in Rust.

## Key Principles

**Keep things simple and efficient unless instructed otherwise.** This codebase prioritizes:
- Performance over feature complexity
- Clear, readable code over clever abstractions
- Verifiable via testing
- DRY (Don't Repeat Yourself) - extract common patterns into shared modules
- Modular files - avoid large files, split functionality into focused modules

## Architecture & Key Components

### Core Components
- **ODGM** (`data-plane/oprc-odgm/`): Main object data grid manager with distributed sharding
- **Gateway** (`data-plane/oprc-gateway/`): HTTP/gRPC gateway for external requests  
- **Router** (`data-plane/oprc-router/`): Zenoh-based message routing
- **Package Manager** (`control-plane/oprc-pm/`): Control plane in Rust (complementing Java CP)
- **Commons** (`commons/`): Shared libraries for storage, models, gRPC, Zenoh integration

### Communication Stack
- **Zenoh**: Primary pub/sub messaging system (not MQTT/Redis)
- **ZRPC**: Custom RPC layer built on Zenoh via `flare-zrpc` library
- **gRPC**: External API interfaces using Protocol Buffers

### Key Networking Patterns
```rust
// Zenoh session creation (standard pattern)
let z_config = oprc_zenoh::OprcZenohConfig::init_from_env()?;
let session = zenoh::open(z_config.create_zenoh()).await?;

// Object invocation routing pattern
let routable = Routable { cls, func, partition };
let mut conn = conn_manager.get_conn(routable).await?;
```

## Essential Development Patterns

### Configuration Management
All services use `envconfig` with consistent patterns:
```rust
#[derive(Envconfig, Clone, Debug)]
pub struct MyConfig {
    #[envconfig(from = "MY_PORT", default = "8080")]
    pub port: u16,
}
let config = MyConfig::init_from_env()?;
```

### Storage Backend Pattern
```rust
// All storage implements ApplicationDataStorage trait
impl ApplicationDataStorage for MyStorage {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()>;
}
```

### Service Architecture
Services follow consistent patterns with Arc-wrapped managers:
```rust
let service = Arc::new(PackageService::new(storage, deployment_service));
let app_state = AppState { package_service, deployment_service, crm_manager };
```

## Build & Development

### Essential Commands
```bash
# Build release version
cargo build -r

# Container development with Just
just dev-up              # Build and start dev containers  
just compose-dev          # Start development environment
just build-release        # Build release containers

# Testing
cargo test                # Unit tests
cargo test --package oprc-pm --test integration_tests  # Integration tests
```

### Dependency Management
This is a Cargo workspace with shared dependencies defined in root `Cargo.toml`:
- **Add new dependencies**: Update `[workspace.dependencies]` in root `Cargo.toml` first
- **Use workspace crates**: Reference internal crates as `oprc-pb = {workspace = true}`
- **External crates**: Add to workspace dependencies, then reference in individual `Cargo.toml`
- **Version consistency**: All workspace members use unified dependency versions

```toml
# In root Cargo.toml [workspace.dependencies]
new-crate = "1.0"

# In individual crate Cargo.toml [dependencies]  
new-crate = {workspace = true}
oprc-models = {workspace = true}
```

### Development Environment
- Use `just` for common workflows (see `justfile`)
- Docker Compose for full system testing
- Individual services can run standalone for development
- `oprc-cli` tool for interacting with the system

### Configuration
- Environment-based config via `envconfig` crate
- Zenoh peer discovery through `OPRC_ZENOH_PEERS` env var
- Service-specific env vars: `ODGM_*`, `OPRC_PM_*`, `HTTP_PORT`, etc.

## Critical Implementation Notes

### Testing Patterns
- Use `#[tokio::test(flavor = "multi_thread")]` for async tests
- Integration tests use `#[serial]` from `serial_test` crate
- Test setup with in-memory storage: `unsafe { std::env::set_var("STORAGE_TYPE", "memory"); }`
- Mock handlers and real services in integration tests
- Use `test-log` crate for tracing in tests: `#[test_log::test]` or `#[tokio::test]` with `#[test_log::test]`

### Logging & Observability
Standard tracing setup across services:
```rust
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(EnvFilter::builder().with_env_var("SERVICE_LOG").from_env_lossy())
    .init();
```
- **Always add tracing**: Use `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!` throughout code
- **Service-specific log levels**: `ODGM_LOG`, `OPRC_LOG`, `SERVICE_LOG` environment variables
- **Test logging**: Use `test-log` crate for visible logs during testing

## Key Files for Reference
- `data-plane/oprc-odgm/src/lib.rs`: ODGM main server setup
- `data-plane/oprc-gateway/src/handler/`: REST and gRPC routing
- `commons/oprc-zenoh/src/lib.rs`: Zenoh configuration patterns
- `control-plane/oprc-pm/tests/integration_tests.rs`: Testing patterns
- `justfile`: Development workflow commands

When working with this codebase, prioritize understanding the Zenoh communication patterns and consistent service architecture patterns that are used across all components.
