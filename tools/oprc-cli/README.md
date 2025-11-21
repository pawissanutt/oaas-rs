# OPRC CLI - Oparaca Command Line Interface

A comprehensive command-line interface for the Oparaca (OaaS) platform, providing both low-level object operations and high-level package management capabilities.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Configuration](#configuration)
- [Commands](#commands)
  - [Context Management](#context-management)
  - [Package Management](#package-management)
  - [Class Management](#class-management)
  - [Function Management](#function-management)
  - [Deployment Management](#deployment-management)
  - [Class Runtime Management](#class-runtime-management)
  - [Deployment Status](#deployment-status)
  - [Environment Listing](#environment-listing)
  - [Object Operations](#object-operations)
  - [Invocation Operations](#invocation-operations)
  - [Result Operations](#result-operations)
  - [Liveliness Operations](#liveliness-operations)
- [Output Formats](#output-formats)
- [Examples](#examples)
- [Migration Guide](#migration-guide)
- [Troubleshooting](#troubleshooting)

## Overview

OPRC CLI integrates OCLI (Oparaca CLI) functionality with the existing OPRC commands, providing a unified interface for:

- **Package Management** - Deploy and manage application packages
- **Class Management** - List and manage deployed classes
- **Function Management** - List and invoke functions
- **Context Management** - Configure and switch between environments
- **Deployment Management** - Monitor and manage deployments
- **Class Runtime Management** - List/get class runtimes (formerly Deployment Records)
- **Deployment Status** - Query live status/state by deployment ID
- **Environment Listing** - List environments (CRM instances) known to the Package Manager
- **Object Operations** - Low-level object CRUD operations (existing)
- **Invocation Operations** - Direct function invocation (existing)

## Installation

### Install with Cargo

```bash
cargo install --bin oprc-cli
```

### Add to PATH

```bash
# Linux/macOS
export PATH="$PATH:$HOME/.cargo/bin"

# Windows (PowerShell)
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
```

## Configuration

OPRC CLI uses a context-based configuration system that stores settings in `$HOME/.oprc/config.yml`.

NOTE: Configure `pmUrl` as the base host (e.g. `http://localhost:8080`) without `/api/v1`; the CLI automatically prefixes PM calls with `/api/v1`.

### Configuration File Structure

```yaml
contexts:
  default:
    pmUrl: "http://pm.oaas.127.0.0.1.nip.io"
    gatewayUrl: "http://oaas.127.0.0.1.nip.io"
    defaultClass: "example.record"
    zenohPeer: "tcp/127.0.0.1:7447"
  production:
    pmUrl: "https://pm.prod.example.com"
    gatewayUrl: "https://api.prod.example.com"
    defaultClass: "prod.main"
    zenohPeer: "tcp/prod.zenoh:7447"
currentContext: "default"
```

### Environment Variables

- `OPRC_CONFIG_PATH` - Override the default config file location
- `OPRC_LOG` - Set logging level (existing)

## Commands

### Context Management

Manage configuration contexts for different environments.

```bash
# Set context configuration
oprc-cli context set [OPTIONS]
oprc-cli ctx s [OPTIONS]  # Short alias

# Display current configuration
oprc-cli context get
oprc-cli ctx g  # Short alias

# Switch between contexts
oprc-cli context select <CONTEXT_NAME>
```

#### Context Set Options

```bash
oprc-cli context set \
  --name production \
  --pm "https://pm.prod.example.com" \
  --gateway "https://api.prod.example.com" \
  --cls "prod.main" \
  --zenoh-peer "tcp/prod.zenoh:7447"
```

- `--name` - Context name (defaults to current)
- `--pm` - Package manager URL
- `--gateway` - Gateway URL for API access
- `--cls` - Default class name
- `--zenoh-peer` - Zenoh peer endpoint

### Package Management

Deploy and manage application packages using YAML definitions.

```bash
# Deploy/update a package
oprc-cli package apply <YAML_FILE> [OPTIONS]
oprc-cli pkg a <YAML_FILE> [OPTIONS]  # Short alias

# Delete a package
oprc-cli package delete <YAML_FILE> [OPTIONS]
oprc-cli pkg d <YAML_FILE> [OPTIONS]  # Short alias
```

#### Package Apply/Delete Options

- `-p, --override-package <NAME>` - Override package name from YAML
- `--apply-deployments` - After applying the package, automatically apply any `deployments` defined inside the YAML



### Class Management

List and manage deployed classes.

```bash
# List all classes
oprc-cli class list [CLASS_NAME]
oprc-cli cls l [CLASS_NAME]  # Short alias

# Delete a class
oprc-cli class delete <CLASS_NAME>
oprc-cli cls delete <CLASS_NAME>  # Short alias
```

### Function Management

List available functions.

```bash
# List all functions
oprc-cli function list [FUNCTION_NAME]
oprc-cli fn l [FUNCTION_NAME]  # Short alias
```

### Deployment Management

Monitor and manage deployments.

```bash
# List deployments
oprc-cli deploy list [DEPLOYMENT_NAME]
oprc-cli dep l [DEPLOYMENT_NAME]  # Short alias

# Apply deployments from an OPackage YAML (posts each OClassDeployment)
oprc-cli deploy apply <YAML_FILE> [--overwrite] [-p <OVERRIDE_PACKAGE>]
oprc-cli dep a <YAML_FILE> --overwrite -p my-pkg  # Short alias

# Delete a deployment
oprc-cli deploy delete <DEPLOYMENT_NAME>
oprc-cli dep delete <DEPLOYMENT_NAME>  # Short alias
```

### Class Runtime Management

List or fetch class runtimes (canonical; replaces "deployment records").

```bash
# List all class runtimes
oprc-cli class-runtimes           # Canonical
oprc-cli runtimes                 # Alias

# Get a specific class runtime by ID
oprc-cli class-runtimes <RUNTIME_ID>
oprc-cli rts <RUNTIME_ID>         # Alias

# Backward-compatible aliases (deprecated):
oprc-cli deployment-records [ID]
oprc-cli recs | rec [ID]
```

### Deployment Status

Fetch the current status for a deployment by its ID.

```bash
oprc-cli deployment-status <DEPLOYMENT_ID>
oprc-cli ds <DEPLOYMENT_ID>  # Short alias
```

### Environment Listing

List environments registered / visible to the Package Manager.

```bash
oprc-cli envs          # Canonical (env-first)
oprc-cli clusters      # Legacy alias (still supported)
oprc-cli env           # Alias
oprc-cli clu           # Alias
oprc-cli cl            # Alias
```

### Object Operations

Low-level object CRUD operations (existing functionality).

```bash
# Object operations
oprc-cli object <OPERATION>
oprc-cli obj <OPERATION>  # Short alias
oprc-cli o <OPERATION>    # Shorter alias
```

### Invocation Operations

Direct function invocation (existing functionality).

```bash
# Invoke functions
oprc-cli invoke <OPTIONS>
oprc-cli ivk <OPTIONS>  # Short alias
oprc-cli i <OPTIONS>    # Shorter alias
```

### Result Operations

Retrieve async operation results (existing functionality).

```bash
# Get results
oprc-cli result <OPTIONS>
oprc-cli res <OPTIONS>  # Short alias
oprc-cli r <OPTIONS>    # Shorter alias
```

### Liveliness Operations

Check environment liveliness (existing functionality).

```bash
# Check liveliness
oprc-cli liveliness
oprc-cli l  # Short alias
```

## Output Formats

OPRC CLI supports multiple output formats for data presentation:

- **JSON** (default) - Machine-readable format
- **YAML** - Human-readable format  
- **Table** - Formatted table output

```bash
# Examples with different output formats
oprc-cli class list -o json   # JSON output (default)
oprc-cli class list -o yaml   # YAML output
oprc-cli class list -o table  # Table output
```

---

## Current Capabilities (TL;DR)
| Area | Implemented | Summary |
|------|-------------|---------|
| Context management | ✅ | Create / update / select named contexts persisted to config file. |
| Package apply/delete | ✅ | Apply (create/update) and delete packages from YAML spec. |
| Class listing/delete | ✅ | List classes, delete by name. |
| Function listing | ✅ | Enumerate functions across packages. |
| Deployment listing/delete | ✅ | List logical deployments and delete by key. |
| Class runtimes list/get | ✅ | List or fetch class runtimes. |
| Deployment status lookup | ✅ | Query status for a deployment ID. |
| Environment listing | ✅ | List environments known to PM. |
| Low-level object ops | ✅ (legacy) | CRUD + scan operations via Zenoh / data plane. |
| Invocation | ✅ | Direct function invocation with parameters. |
| Async result retrieval | ✅ | Fetch results of previously invoked async operations. |
| Liveliness check | ✅ | Basic liveliness / health probe. |
| Multiple output formats | ✅ | json / yaml / table selection. |
| Short aliases | ✅ | Compact verb + noun shorthand (e.g. `cls l`). |
| Config override via env | ⚠️ (minimal) | `OPRC_CONFIG_PATH` only; log level env pass‑through. |
| Structured errors | ⚠️ (basic) | Simple error conversions; limited categorization. |
| Tests | ✅ (initial) | `tests/cli_pm_integration.rs` covers package/class/function flows. |

Legend: ✅ done • ⚠️ partial / basic • ⏳ planned • ❌ not started

---

## Roadmap / Milestones
Mirrors style of service READMEs. Each milestone groups logically incremental user value.

### M1 Baseline unification (DONE)
- [x] Merge legacy object/invocation commands with new PM facing commands.
- [x] Context CRUD + selection with persisted YAML.
- [x] Package apply/delete (YAML) with name override flag.
- [x] Class / function / deployment list + delete commands; class runtime list/get.
- [x] Invocation + result retrieval parity with prior tool.
- [x] Output formatting (json|yaml|table) and short aliases.
- [x] Basic integration test hitting PM endpoints (`cli_pm_integration`).


### M2 Multi‑environment & Observability Integration
- [ ] Display per‑environment deployment status summary (`deploy list --envs`).
- [ ] Add `environment health` command (fan‑out to PM / CRM aggregated endpoint).
- [ ] Watch/stream mode for deployments (`deploy watch <key>` with live status).
- [ ] Function latency & invocation count summary (if PM exposes metrics endpoint).
- [ ] Optional progress spinners for long operations.


### M3 Usability & Safety
- [ ] Dry‑run mode (`--dry-run`) for package/deployment apply (schema + diff, no submit).
- [ ] Rich diff preview for `package apply` (show changed classes/functions/deployments).
- [ ] Auto-completion script generation (bash/zsh/fish/pwsh) `oprc-cli completion <shell>`.
- [ ] Inline help examples per subcommand (succinct, copy‑paste ready).
- [ ] Improved error taxonomy (network vs validation vs server) with exit codes.
- [ ] Colored / styled table output (respect `NO_COLOR`).
- [ ] Config validation command `oprc-cli config validate`.
- [ ] Global `--timeout` and `--retries` flags.

### M5 Security & Profiles
- [ ] Pluggable auth: API token / mTLS (config + flags).
- [ ] Secrets masking in logs / output.
- [ ] Context import/export (tar/zip) for sharing team configs.
- [ ] Encrypted context fields (symmetric key or DPAPI on Windows).

### M6 Packaging & Distribution
- [ ] Prebuilt binaries (GitHub Releases) with checksum/signature.
- [ ] Homebrew tap / Scoop manifest / Cargo install docs refresh.
- [ ] Minimal Docker image for CI usage.

### Stretch / Future
- [ ] Interactive TUI mode (status dashboard for packages & deployments).
- [ ] Scenario scripts (`oprc-cli generate example --type echo` scaffolding sample YAML).
- [ ] Offline bundle: package + functions + metadata archived & replayable.
- [ ] Telemetry opt‑in (anonymous usage stats) with `--no-telemetry` override.
- [ ] Plugin system (dynamic discovery of extra subcommands).
- [ ] AI assist integration for command suggestion/error remediation.

---

## Feature Design Notes

### Dry-run & Diff
Contract: Accept same inputs as `package apply`; resolve current server state; compute semantic diff (add/change/remove) across classes, functions, deployments. Output structured JSON (machine) or human diff table (table format). Exit code 0 when differences shown; 2 if no changes; >2 on error.

Edge cases:
* Package not present (full create diff).
* Renamed class/function (delete + create vs rename heuristic) — initial implementation treats as delete/create.
* Large packages: diff must paginate or collapse unchanged sections on table output.

### Watch Mode
Uses periodic polling (initial) with exponential backoff ceiling. Later may upgrade to server push / SSE if exposed. Provides transitions with timestamps and final summary.

### Bulk Object Load
Input formats: JSON Lines (one object per line) and CSV (schema inferred or provided with `--schema`). Batching size flag; parallelism limit to avoid overload. Reports success/failure counts and first N error samples.

### Auth Model
Config fields: `auth.token`, `auth.mtls.ca`, `auth.mtls.cert`, `auth.mtls.key`. Command flags override context for a single invocation. Token auto‑read from `OPRC_TOKEN` env if unset in context.

---

## Testing Strategy
| Layer | Current | Planned |
|-------|---------|---------|
| Unit | Basic parsing/enum tests | Expand for diff engine, config validation |
| Integration | PM CRUD + list smoke | Add retry / error path tests, watch mode simulation |
| E2E (optional) | Manual via scripts | Scripted multi‑environment scenario with fixtures |

Planned helpers in `commands/tests.rs` for table parsing & golden output comparisons.

---

## Configuration (Extended)
Additional planned env variables:
| Env | Purpose |
|-----|---------|
| `OPRC_TIMEOUT_SECS` | Default HTTP/gRPC client timeout for PM / gateway calls. |
| `OPRC_RETRIES` | Global default retry attempts for transient network errors. |
| `OPRC_OUTPUT` | Default output format override (json|yaml|table). |
| `OPRC_TOKEN` | Bearer token for auth (if feature enabled). |

Priority resolution order (planned): flag > env > context config > default.

---

## Contribution Guidelines (CLI Specific)
1. Add or adjust command: update `commands/<domain>.rs` and central dispatch in `commands/mod.rs`.
2. Provide help text & example usage; keep examples under 100 chars width.
3. Include unit tests for argument parsing; integration test for end‑to‑end server interaction.
4. Update roadmap checklist when completing or adding scope.
5. Run `cargo fmt && cargo clippy --all-targets -- -D warnings` before PR.

---

## Quick Reference
| Task | Command |
|------|---------|
| Apply package | `oprc-cli package apply pkg.yaml` |
| Delete package | `oprc-cli package delete pkg.yaml` |
| List classes | `oprc-cli class list -o table` |
| Invoke function | `oprc-cli invoke --class cls --function f --data '{"x":1}'` |
| Watch deployment (planned) | `oprc-cli deploy watch dep1` |
| Diff package (planned) | `oprc-cli package apply pkg.yaml --dry-run --diff` |

---

## Status
Baseline (M1) complete. Actively implementing M2 usability features. See roadmap above for progress.

---

## References
* Service READMEs: [Package Manager](../../control-plane/oprc-pm/README.md), [Class Runtime Manager](../../control-plane/oprc-crm/README.md)
* Architecture docs: [Control Plane](../../docs/CONTROL_PLANE.md), [Data Plane](../../docs/DATA_PLANE.md)
* NFR Enforcement Design: [NFR Enforcement](../../docs/NFR_ENFORCEMENT_DESIGN.md)
* ODGM Overview: see `data-plane/oprc-odgm/README.adoc`
* gRPC Protos: `commons/oprc-grpc/proto`
* Models (types): `commons/oprc-models`
