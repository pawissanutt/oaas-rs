# ODGM Integration Tests

This directory contains comprehensive integration tests for the ODGM (Object Data Grid Manager) component.

## Test Structure

### Test Modules

1. **`integration_test.rs`** - Main integration tests covering basic ODGM functionality
2. **`data_operations_test.rs`** - Tests for CRUD operations and data handling
3. **`collection_management_test.rs`** - Tests for collection creation, management, and lifecycle
4. **`cluster_test.rs`** - Tests for cluster operations, replication, and distributed functionality
5. **`event_system_test.rs`** - Tests for event system functionality (when API is available)
6. **`performance_test.rs`** - Performance and stress tests

### Common Module

The `common/` directory contains shared utilities:
- `TestEnvironment` - Manages ODGM instances and gRPC servers for testing
- `TestConfig` - Configuration management for test environments
- `test_data` - Helper functions for creating test data
- `assertions` - Custom assertion helpers
- `setup` - Utilities for setting up different test scenarios

## Running Tests

### Run All Tests
```bash
cargo test --package oprc-odgm
```

### Run Specific Test Modules
```bash
# Basic integration tests
cargo test --package oprc-odgm integration_test

# Data operations tests
cargo test --package oprc-odgm data_operations_test

# Collection management tests
cargo test --package oprc-odgm collection_management_test

# Cluster tests
cargo test --package oprc-odgm cluster_test

# Event system tests (placeholders until API is available)
cargo test --package oprc-odgm event_system_test

# Performance tests
cargo test --package oprc-odgm performance_test
```

### Run Tests with Output
```bash
cargo test --package oprc-odgm -- --nocapture
```

### Run Tests in Single Thread (for debugging)
```bash
cargo test --package oprc-odgm -- --test-threads=1
```

## Test Categories

### 1. Basic Functionality Tests
- ODGM server startup and shutdown
- Collection creation and management
- Basic CRUD operations via gRPC
- Configuration validation

### 2. Data Operations Tests
- Object creation, retrieval, update, deletion
- Key-value operations
- Complex object handling
- Batch operations
- Error handling for non-existent data

### 3. Collection Management Tests
- Creating collections with different parameters
- Multiple collection handling
- Partition and replica configuration
- Collection validation and error handling

### 4. Cluster Tests
- Multi-node cluster formation
- Shard distribution across nodes
- Node failure and recovery scenarios
- Cluster consensus and consistency
- Load balancing verification

### 5. Event System Tests
- Event system initialization
- Event configuration management
- Event trigger management (when API is available)
- Event cascading and depth limits
- Event timeout handling
- Cluster event propagation

### 6. Performance Tests
- High-throughput read/write operations
- Mixed workload scenarios
- Large object handling
- Memory usage under load
- Concurrent operations across multiple collections

## Test Environment

### Prerequisites
- Zenoh broker (managed automatically by test environment)
- Available network ports for gRPC servers
- Sufficient system resources for concurrent tests

### Configuration
Tests use environment-specific configurations:
- Random node IDs to avoid conflicts
- Dynamic port allocation
- Isolated test environments
- Automatic cleanup on test completion

### Resource Management
- Each test creates its own ODGM instance
- Automatic port allocation prevents conflicts
- Graceful shutdown ensures cleanup
- Resource isolation between tests

## Test Data

Tests use standardized test data patterns:
- Simple objects with string values
- Complex objects with multiple data types
- Large objects for performance testing
- Predictable key patterns for verification

## Debugging Tests

### Verbose Logging
Set environment variable for detailed logs:
```bash
ODGM_LOG=debug cargo test --package oprc-odgm
```

### Single Test Execution
```bash
cargo test --package oprc-odgm test_odgm_startup_shutdown -- --exact
```

### Test with Tracing
The tests use `tracing-test` for detailed execution tracing:
```bash
cargo test --package oprc-odgm -- --nocapture
```

## Known Limitations

1. **Event System Tests**: Many event system tests are currently placeholders waiting for the event API to be fully implemented.

2. **Network Simulation**: Network partition tests simulate failures by shutting down nodes rather than actual network partitioning.

3. **Performance Thresholds**: Performance test thresholds may need adjustment based on hardware capabilities.

4. **Cluster Size**: Cluster tests are limited to small cluster sizes (2-3 nodes) to reduce test complexity.

## Future Enhancements

1. **Event API Integration**: Complete event system tests when the event API is available
2. **Network Simulation**: Implement true network partition simulation
3. **Load Testing**: Add more comprehensive load testing scenarios
4. **Monitoring Integration**: Add metrics collection and analysis
5. **Chaos Testing**: Implement chaos engineering tests for resilience validation

## Contributing

When adding new tests:
1. Use the `TestEnvironment` for consistent setup
2. Follow the existing naming conventions
3. Include proper cleanup in all tests
4. Add performance assertions where appropriate
5. Document any special requirements or limitations
