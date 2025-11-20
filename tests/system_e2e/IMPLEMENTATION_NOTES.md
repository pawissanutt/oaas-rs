# System E2E Test Suite - Implementation Notes

## Overview

This document provides implementation details and design decisions for the system E2E test suite.

## Architecture

The test suite is implemented as a standalone Rust crate (`system_e2e`) that orchestrates the complete testing lifecycle:

1. **Infrastructure Setup** - Provisions Kind cluster, builds and loads Docker images
2. **Deployment** - Deploys OaaS components using existing deployment scripts
3. **Test Execution** - Runs test scenarios using `oprc-cli` and K8s API
4. **Validation** - Asserts system state and function invocation responses
5. **Teardown** - Cleans up Kind cluster and resources

## Key Design Decisions

### 1. Separate Crate Structure

The E2E tests are implemented as a separate crate rather than being embedded in existing packages because:
- Clear separation of concerns (system tests vs unit/integration tests)
- Independent dependency management (can use different versions if needed)
- Easier to exclude from regular builds/tests (E2E tests are expensive)
- Better organization of test fixtures and helpers

### 2. Kind for Local Kubernetes

We use Kind (Kubernetes in Docker) because:
- Reproducible - Creates identical clusters on any developer machine or CI
- Fast setup/teardown - Clusters can be created/destroyed in seconds
- Self-contained - No external dependencies or cloud credentials needed
- CI-friendly - Runs in GitHub Actions and other CI systems

### 3. Process Execution with `duct`

We use the `duct` crate for running external commands because:
- Better ergonomics than `std::process::Command`
- Easy output capture and chaining
- Good error handling and status code checking
- Widely used in the Rust ecosystem

### 4. oprc-cli Integration

Instead of directly calling REST/gRPC APIs, we use `oprc-cli` because:
- Tests the actual user experience (CLI is the primary interface)
- Validates CLI functionality alongside the platform
- Simpler than managing HTTP clients and authentication
- Easier to debug (can run the same commands manually)

### 5. Ignored by Default

E2E tests are marked with `#[ignore]` because:
- They require external dependencies (Kind, Docker, kubectl)
- They're slow (minutes vs milliseconds for unit tests)
- They modify system state (create clusters, load images)
- Should be opt-in for developers, but run in CI

### 6. Cleanup with Drop Guards

We use Drop guards for cleanup because:
- Ensures cleanup happens even if tests panic
- Prevents resource leaks in CI and local development
- Can be disabled for debugging with `E2E_SKIP_CLEANUP`

## Module Structure

### `lib.rs`
- Shared constants (cluster name, image list)
- Utility functions (repo root detection, cleanup control)
- Module declarations

### `setup.rs`
- Infrastructure provisioning (Kind cluster creation)
- Image building and loading
- OaaS deployment orchestration
- Cleanup operations

### `cli.rs`
- CLI wrapper for `oprc-cli` commands
- Configuration management
- Command execution with proper error handling
- Output parsing (JSON, plain text)

### `k8s_helpers.rs`
- Kubernetes state validation
- Pod readiness waiting with timeouts
- Service endpoint discovery
- Namespace checks

### `scenarios/basic_flow.rs`
- Main E2E test harness
- Setup/teardown orchestration
- Test scenario implementation
- Comprehensive logging

### `fixtures/`
- Test YAML files (packages, deployments)
- Minimal examples that work with the test infrastructure

## Testing Strategy

### What We Test

1. **Package Management**
   - Apply package YAML via CLI
   - Verify package exists in PM
   - Get package info

2. **Deployment**
   - Apply deployment YAML via CLI
   - Wait for ODGM pods to become ready
   - Verify K8s resources are created

3. **Invocation**
   - Invoke function via CLI
   - Verify response contains expected data

### What We Don't Test (Yet)

- Function update scenarios
- Scale-up/scale-down
- Multi-environment deployments
- Error cases (invalid YAML, missing dependencies)
- Performance/load testing

These could be added as additional test scenarios in the future.

## CI Integration

The GitHub Actions workflow (`.github/workflows/e2e-tests.yml`) runs on:
- Pull requests to main
- Pushes to main
- Manual trigger (workflow_dispatch)

It includes:
- Rust toolchain setup
- Cargo caching for faster builds
- Kind and kubectl installation
- Docker validation
- Test execution with proper logging
- Cleanup on failure

## Debugging Tips

### Keep Cluster Running After Test

```bash
E2E_SKIP_CLEANUP=1 cargo test --package system_e2e --test e2e_basic_flow -- --ignored --nocapture
```

Then inspect with:

```bash
kubectl config use-context kind-oaas-e2e
kubectl get pods -A
kubectl logs -n oaas <pod-name>
```

### Enable Debug Logging

```bash
RUST_LOG=debug cargo test --package system_e2e -- --ignored --nocapture
```

### Manual Cleanup

```bash
kind delete cluster --name oaas-e2e
```

## Future Improvements

### Short Term
1. Add more test scenarios (error cases, updates, deletions)
2. Improve fixture management (templates, parameterization)
3. Add metrics validation (Prometheus queries)
4. Test ingress configuration

### Medium Term
1. Parallel test execution (multiple clusters)
2. Test result reporting (JUnit XML, HTML reports)
3. Performance benchmarks
4. Chaos testing (pod kills, network partitions)

### Long Term
1. Multi-cluster testing
2. Upgrade testing (version N to N+1)
3. Integration with existing test infrastructure
4. Test data generation and management

## Common Issues

### Issue: Tests timeout waiting for pods

**Cause**: Slow image pulls, insufficient resources, or networking issues

**Solution**:
- Increase timeout values in `k8s_helpers.rs`
- Check Docker resources (CPU/Memory)
- Pre-pull images before running tests
- Check network connectivity

### Issue: Kind cluster creation fails

**Cause**: Port conflicts, existing cluster, Docker issues

**Solution**:
```bash
# Delete existing cluster
kind delete cluster --name oaas-e2e

# Check Docker
docker ps

# Check ports
lsof -i :6443  # Kind API server port
```

### Issue: oprc-cli not found

**Cause**: CLI not built or not in PATH

**Solution**:
```bash
# Build CLI first
cargo build -p oprc-cli

# Or install globally
cargo install --path tools/oprc-cli
```

### Issue: Image not found in Kind

**Cause**: Images not loaded or wrong tag

**Solution**:
- Ensure images are built with correct tags
- Check `load_all_images()` is called in setup
- Verify image names match `REQUIRED_IMAGES` constant

## Contributing

When adding new E2E tests:

1. Follow the existing pattern in `basic_flow.rs`
2. Use the shared helpers from `lib.rs`, `setup.rs`, `cli.rs`, `k8s_helpers.rs`
3. Add comprehensive logging (info level for progress, debug for details)
4. Clean up resources in Drop guards
5. Document test purpose and prerequisites
6. Update this document with new scenarios

## References

- [Kind Documentation](https://kind.sigs.k8s.io/)
- [kube-rs Documentation](https://docs.rs/kube/)
- [duct Documentation](https://docs.rs/duct/)
- [OaaS-RS Architecture](../../README.adoc)
