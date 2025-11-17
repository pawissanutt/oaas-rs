# OPRC Zenoh Utilities

Lightweight helpers to configure and manage Zenoh sessions consistently across the OaaS-RS monorepo.

## Highlights
- Env-first configuration via `envconfig` and sensible defaults
- Ephemeral-port friendly for tests (port `0`)
- Optional shared-session mode for in-process multi-node tests
- Utilities to declare subscribers and queryables with worker pools

## Quick start

```rust
use oprc_zenoh::OprcZenohConfig;
use zenoh::prelude::r#async::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build config from environment and open a session
    let cfg = OprcZenohConfig::init_from_env()?;
    let session = zenoh::open(cfg.create_zenoh()).await?;

    // ... use the `session` to publish/subscribe/query ...

    session.close().await?;
    Ok(0.into())
}
```

## Environment variables

Common variables understood by this crate (and respected across services):

- `OPRC_ZENOH_PORT` (default: `0`)
  - Listening port. `0` means OS-ephemeral to avoid collisions in tests.
- `OPRC_ZENOH_PROTOCOL` (default: `tcp`)
  - Transport protocol.
- `OPRC_ZENOH_PEERS` (default: none)
  - Comma-separated peer locators.
- `OPRC_ZENOH_MODE` (default: `peer`)
  - One of `peer`, `client`, or `router`.
- `OPRC_ZENOH_GOSSIP_ENABLED` (auto: `true` when using ephemeral port)
  - Enables gossip-based scouting to make ephemeral, in-process discovery easy.
- `OPRC_ZENOH_SHARE_SESSION` (default: disabled)
  - When set to `1`/`true`, the pool reuses a single session in-process and makes `Pool::close()` a no-op. Useful for multi-node tests that must not tear down the session prematurely.

## Behavior notes

- Ephemeral ports + gossip are ideal for test runs and local multi-process setups.
- If you need deterministic ports, explicitly set `OPRC_ZENOH_PORT` and optionally disable gossip (`OPRC_ZENOH_GOSSIP_ENABLED=false`).
- In shared-session mode, rely on process shutdown or explicit pool drop as the teardown signal; individual components calling `close()` won’t actually close the underlying session.

## Tracing

This crate uses the repo-wide `tracing` setup. Control verbosity via `RUST_LOG` (e.g., `RUST_LOG=info,zenoh=info,oprc_zenoh=debug`).
