# System E2E Tests

Automated end-to-end test suite for OaaS-RS that validates the entire platform on a local Kubernetes cluster.

## Overview

This test suite orchestrates the complete lifecycle:
1. **Infrastructure**: Provision Kind cluster, build images, load images
2. **Deployment**: Deploy OaaS components (PM, CRM, Gateway) using Helm/Manifests
3. **Execution**: Run test scenarios using `oprc-cli` commands
4. **Validation**: Assert system state via K8s API and function invocation responses

## Prerequisites

The following tools must be installed and available in your PATH:
- `kind` - Kubernetes in Docker
- `kubectl` - Kubernetes CLI
- `docker` - Container runtime
- `just` - Command runner (optional, for convenience)

## Running Tests

### Run All E2E Tests

```bash
# Build the project first
cargo build --release

# Run all E2E tests (they are ignored by default)
cargo test --package system_e2e -- --ignored --nocapture
```

### Run Specific Test

```bash
# Run the basic flow test
cargo test --package system_e2e --test e2e_basic_flow -- --ignored --nocapture
```

### Environment Variables

- `RUST_LOG` - Set log level (default: `info`)
  ```bash
  RUST_LOG=debug cargo test --package system_e2e -- --ignored --nocapture
  ```

- `E2E_SKIP_CLEANUP` - Skip cluster cleanup for debugging
  ```bash
  E2E_SKIP_CLEANUP=1 cargo test --package system_e2e -- --ignored --nocapture
  ```

## Test Structure

```
tests/system_e2e/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs              # Shared utilities and constants
    ├── setup.rs            # Infrastructure setup (Kind, images, deploy)
    ├── cli.rs              # CLI wrapper for oprc-cli commands
    ├── k8s_helpers.rs      # Kubernetes state validation helpers
    ├── fixtures/           # Test YAML files
    │   ├── example_package.yaml
    │   └── example_deployment.yaml
    └── scenarios/          # Test scenarios
        └── basic_flow.rs   # Basic happy path test
```

## Test Scenarios

### Basic Flow (`basic_flow.rs`)

Tests the complete happy path:
1. Apply a test package to PM
2. Deploy the package to a target environment
3. Wait for ODGM pods to become ready
4. Invoke a function and validate the response

## Debugging

### Keep Cluster Running After Test

```bash
E2E_SKIP_CLEANUP=1 cargo test --package system_e2e --test e2e_basic_flow -- --ignored --nocapture
```

Then you can inspect the cluster:

```bash
# Switch to the e2e cluster context
kubectl config use-context kind-oaas-e2e

# List pods
kubectl get pods -A

# Check logs
kubectl logs -n oaas <pod-name>
```

### Clean Up Manually

```bash
kind delete cluster --name oaas-e2e
```

## Adding New Tests

1. Create a new test file in `src/scenarios/`
2. Add the test to `Cargo.toml`:
   ```toml
   [[test]]
   name = "e2e_my_test"
   path = "src/scenarios/my_test.rs"
   harness = true
   ```
3. Follow the pattern from `basic_flow.rs`
4. Use the shared helpers from `lib.rs`, `setup.rs`, `cli.rs`, and `k8s_helpers.rs`

## CI Integration

These tests are designed to run in CI pipelines. Example GitHub Actions workflow:

```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Kind
        run: |
          curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
          chmod +x ./kind
          sudo mv ./kind /usr/local/bin/kind
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Build
        run: cargo build --release
      
      - name: Run E2E Tests
        run: cargo test --package system_e2e -- --ignored --nocapture
```

## Troubleshooting

### Test Timeout

If tests timeout, increase the timeout values in `k8s_helpers.rs` or check:
- Docker resources (CPU/Memory)
- Network connectivity
- Image pull times

### Image Not Found

Ensure images are built before running tests:
```bash
just build release
```

Or use docker compose:
```bash
docker compose -f docker-compose.build.yml build
```

### Kind Cluster Issues

Check if Kind is working:
```bash
kind version
kind get clusters
```

Delete any existing clusters:
```bash
kind delete cluster --name oaas-e2e
```
