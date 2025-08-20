# Package Manager - Rust Architecture & Implementation Plan

## Overview

The Package Manager serves as the central control plane for the OaaS (Object-as-a-Service) platform, written in Rust for performance, safety, and reliability. This document outlines a pragmatic architecture focusing on MVP implementation while maintaining extensibility.

**Note**: The Package Manager is a REST API service that manages packages and deployments. It does not generate or use Kubernetes CRDs - that functionality belongs to the Class Runtime Manager which manages Kubernetes resources directly.

## Architectural Improvements & Simplifications

### Key Changes from Original Specification

1. **Distributed Storage**: Use etcd for distributed, consistent storage with built-in clustering
2. **Async-First Design**: Leverage Rust's async ecosystem (tokio, axum)
3. **Type Safety**: Use Rust's type system to prevent runtime errors in deployment logic
4. **Modular Architecture**: Clear separation of concerns with well-defined interfaces
5. **Environment-Based Configuration**: Use environment variables for all configuration
6. **Observability Built-in**: Structured logging and metrics from day one

## Core Architecture

### 0. Shared Module Dependencies

The Package Manager depends on shared modules for common functionality:

```
commons/
├── oprc-grpc/           // Shared gRPC definitions and clients
├── oprc-models/         // Common data models and types
├── oprc-cp-storage/     // Control Plane Storage abstractions and implementations
├── oprc-observability/  // Logging, metrics, tracing utilities
└── oprc-config/         // Configuration management utilities
```

### 1. Application Structure

The Package Manager should be organized within the `control-plane` directory alongside the Class Runtime Manager to clearly separate control plane components from data plane components (like ODGM).

```
control-plane/
├── oprc-pm/                     // Package Manager (this component)
└── oprc-crm/                    // Class Runtime Manager (will use CRDs)
```

**Package Manager Directory Structure:**
```
oprc-pm/
├── src/
│   ├── main.rs              // Application entry point
│   ├── lib.rs               // Library exports
│   ├── config/              // Configuration management (envconfig-based)
│   ├── models/              // Data models and schemas
│   ├── storage/             // Data persistence layer (etcd, memory)
│   ├── services/            // Business logic services
│   ├── api/                 // HTTP API layer
│   ├── crm/                 // CRM integration
│   └── observability/       // Logging, metrics, tracing
├── tests/
│   ├── integration/
│   └── fixtures/
├── docker-compose.yml       // etcd testing setup
├── docker-compose.test.yml  // Multi-node etcd cluster
└── justfile                 // Build and testing automation
```

### 2. Data Models

The Package Manager uses gRPC generated types from the shared `oprc-grpc` module to ensure consistency across services and eliminate model duplication.

**Primary Data Types:**
- `OPackage` - Package definitions with metadata and classes
- `OClass` - Class definitions with functions and state specifications  
- `OFunction` - Function definitions with bindings and parameters
- `DeploymentUnit` - Deployment configurations and requirements
- `OClassDeployment` - Class deployment specifications
- `RuntimeState` - Runtime state tracking and metrics

**Extended Types for Package Manager Logic:**
- `DeploymentReplicas` - Replica configuration (min, max, target)
- `PartitionInfo` - Partition configuration and replication
- `RuntimeMetrics` - CPU, memory, request/error counts, latency
- `PackageFilter` / `DeploymentFilter` - Query filtering and pagination

### 3. Storage Layer

**Storage Abstraction:**
The Package Manager uses a trait-based storage abstraction that supports both etcd (for production) and in-memory storage (for testing). All storage operations are async and use the gRPC generated types directly.

**Core Operations:**
- Package CRUD operations with filtering and pagination
- Deployment management and status tracking
- Runtime state updates and monitoring
- Multi-cluster state synchronization

**etcd Implementation:**
Uses the `etcd-rs` client library for distributed storage with features including:
- Key prefixing for namespace isolation
- JSON serialization of gRPC types
- Atomic operations and transactions
- Prefix-based queries for efficient listing
- Watch support for real-time updates

### 4. Service Layer

**Package Service:**
Handles package lifecycle management including validation, dependency resolution, and triggering deployments. Validates package structure, checks dependencies, and coordinates with the deployment service.

**Deployment Service:**
Manages deployment orchestration by calculating requirements, creating deployment units for different environments, and coordinating with CRM instances based on target clusters.

**Key Responsibilities:**
- Package validation and dependency checking
- Deployment requirement calculation
- Multi-cluster deployment coordination
- Runtime state management and health monitoring

### 5. API Layer

**REST API Endpoints:**

**Package Management:**
- `POST /api/v1/packages` - Create a new package
- `GET /api/v1/packages` - List packages with optional filtering
- `GET /api/v1/packages/{name}` - Get specific package details
- `DELETE /api/v1/packages/{name}` - Delete a package

**Deployment Management:**
- `POST /api/v1/deployments` - Create a new deployment
- `GET /api/v1/deployments` - List deployments with filtering

**Deployment Records (from CRM clusters):**
- `GET /api/v1/deployment-records` - List records from all CRM clusters
- `GET /api/v1/deployment-records?cluster=prod-east` - List from specific cluster
- `GET /api/v1/deployment-records/{id}` - Get specific record (searches all clusters)
- `GET /api/v1/deployment-status/{id}` - Get deployment status
- `DELETE /api/v1/deployments/{id}?cluster=prod-east` - Delete deployment

**Multi-cluster Management:**
- `GET /api/v1/clusters` - List all configured clusters with health status
- `GET /api/v1/clusters/{name}/health` - Get health status of specific cluster

**Query APIs:**
- `GET /api/v1/classes` - List classes across packages
- `GET /api/v1/functions` - List functions across packages

**HTTP Server Features:**
- Built with Axum framework for high performance
- Structured logging and metrics middleware
- JSON request/response handling with proper error messages
- Query parameter parsing for filtering and pagination

### 6. CRM Integration

**Multi-Cluster Architecture:**
The Package Manager supports connecting to multiple Class Runtime Manager (CRM) instances, typically one per Kubernetes cluster. This enables multi-cluster deployments where each cluster has its own CRM managing local resources.

**CRM Manager:**
- Manages connections to multiple CRM instances (one per cluster)
- Handles cluster discovery and health monitoring
- Provides automatic failover and load balancing
- Supports default cluster configuration for simplified operations

**CRM Client (Per Cluster):**
- HTTP client for individual CRM communication
- Deployment lifecycle management (create, status, delete)
- Health checking and circuit breaker patterns
- Authentication and TLS support

**Key Features:**
- Multi-cluster deployment coordination
- Health monitoring and automatic failover
- Deployment record aggregation across clusters
- Cluster-specific authentication and configuration
- Circuit breaker pattern for fault tolerance

### 7. Configuration Management

**Environment-Variable-Only Configuration:**
The Package Manager uses `envconfig` for clean, 12-factor app compliant configuration. All configuration is provided through environment variables, eliminating the need for config files in production deployments.

**Key Configuration Areas:**
- **Server Configuration**: Host, port, worker threads
- **Storage Configuration**: etcd endpoints, credentials, TLS settings
- **CRM Configuration**: Multi-cluster CRM endpoints and authentication
- **Observability Configuration**: Log levels, metrics, tracing endpoints

**Configuration Structure:**
```rust
#[derive(Debug, Clone, Envconfig)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub storage: StorageConfig, 
    pub crm: CrmManagerConfig,
    pub observability: ObservabilityConfig,
}
```

**Benefits:**
- Container-friendly deployment
- No config file management overhead
- Environment-specific overrides built-in
- Secure credential handling through environment variables

## Implementation Plan

### Phase 1: Core Foundation (2-3 weeks)
1. **Project Setup**
   - Cargo workspace configuration
   - Basic dependency setup (tokio, serde, axum, etcd-rs)
   - Development environment setup

2. **Data Models**
   - Define core Rust structs with proper serialization
   - Implement validation logic
   - Create error types

3. **Storage Layer**
   - Implement etcd distributed storage
   - Create storage abstraction trait
   - Add basic CRUD operations with etcd integration

### Phase 2: Core Services (2-3 weeks)
1. **Package Service**
   - Package creation and validation
   - Dependency resolution
   - Package lifecycle management

2. **Basic API**
   - HTTP server setup with axum
   - Core endpoints implementation
   - Request/response handling

3. **Configuration**
   - Environment-variable configuration with envconfig
   - Validation and defaults

### Phase 3: Deployment Orchestration (3-4 weeks)
1. **Deployment Service**
   - Deployment planning logic
   - Resource calculation
   - Environment management

2. **CRM Integration**
   - HTTP client implementation
   - Deployment unit creation
   - Multi-cluster support

3. **State Management**
   - Runtime state tracking
   - Health monitoring
   - Recovery mechanisms

### Phase 4: Production Readiness (2-3 weeks)
1. **Observability**
   - Structured logging
   - Metrics collection
   - Health checks

2. **Error Handling**
   - Comprehensive error types
   - Recovery mechanisms
   - Graceful degradation

3. **Testing**
   - Unit tests
   - Integration tests
   - Performance tests

## Key Rust Libraries & Dependencies

### Why etcd for Storage?

etcd provides several advantages for the Package Manager:

1. **Distributed Consistency**: Strong consistency guarantees across multiple Package Manager instances
2. **High Availability**: Built-in clustering and leader election
3. **Watch API**: Real-time notifications for configuration changes
4. **Kubernetes Integration**: Native integration with Kubernetes ecosystem
5. **Performance**: Optimized for metadata storage and frequent reads
6. **Operational Maturity**: Battle-tested in production Kubernetes clusters
7. **Security**: Built-in TLS, authentication, and authorization

### Storage Architecture Benefits

- **Multi-instance Deployment**: Multiple Package Manager instances can share the same etcd cluster
- **Fault Tolerance**: Automatic failover and recovery
- **Backup & Recovery**: Standard etcd backup procedures
- **Scalability**: Read scaling through etcd proxies
- **Consistency**: Strong consistency for critical deployment state

### Core Dependencies
- `tokio` - Async runtime
- `serde` - Serialization/deserialization
- `axum` - HTTP server framework
- `etcd-rs` - etcd client library
- `reqwest` - HTTP client
- `anyhow` - Error handling
- `thiserror` - Custom error types
- `envconfig` - Environment-variable configuration
- `tracing` - Structured logging
- `metrics` - Metrics collection

### Development Dependencies
- `tokio-test` - Testing utilities
- `wiremock` - HTTP mocking
- `testcontainers` - Integration testing
- `criterion` - Benchmarking
- `serial_test` - Test isolation for environment variables

## Deployment Considerations

### Container Setup
```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/oprc-pm /usr/local/bin/
EXPOSE 8080
CMD ["oprc-pm"]
```

### Configuration Management
- Use environment variables for deployment-specific config
- Container-friendly configuration with envconfig
- Kubernetes ConfigMaps and Secrets for sensitive data

#### Example Environment Variables for Production

```bash
# etcd Configuration
OPRC_PM_ETCD_ENDPOINTS="etcd-1:2379,etcd-2:2379,etcd-3:2379"
OPRC_PM_ETCD_USERNAME="oaas-pm"
OPRC_PM_ETCD_PASSWORD="secure-password"
OPRC_PM_ETCD_KEY_PREFIX="/oaas/pm"

# TLS Configuration
OPRC_PM_ETCD_CA_CERT_PATH="/etc/ssl/etcd/ca.pem"
OPRC_PM_ETCD_CLIENT_CERT_PATH="/etc/ssl/etcd/client.pem"
OPRC_PM_ETCD_CLIENT_KEY_PATH="/etc/ssl/etcd/client-key.pem"

# CRM Multi-cluster Configuration
OPRC_PM_CRM_DEFAULT_CLUSTER="production-east"
OPRC_PM_CRM_PROD_EAST_TOKEN="prod-east-bearer-token"
OPRC_PM_CRM_PROD_WEST_TOKEN="prod-west-bearer-token"

# CRM Cluster URLs
OPRC_PM_CRM_PROD_EAST_URL="https://crm-prod-east.internal:8081"
OPRC_PM_CRM_PROD_WEST_URL="https://crm-prod-west.internal:8081" 
OPRC_PM_CRM_STAGING_URL="http://crm-staging.internal:8081"

# Server Configuration
OPRC_PM_SERVER_PORT=8080
OPRC_PM_LOG_LEVEL=info
```

## Security Considerations

1. **Input Validation**: Comprehensive validation of all package inputs
2. **Authentication**: JWT-based authentication for API endpoints
3. **Authorization**: Role-based access control for different operations
4. **Rate Limiting**: Protection against abuse
5. **Audit Logging**: Track all package and deployment operations
6. **etcd Security**: TLS encryption, client certificates, and RBAC

## etcd Operations & Best Practices

### Deployment Patterns

1. **Development**: Single-node etcd for simplicity
2. **Staging**: 3-node etcd cluster for testing HA scenarios
3. **Production**: 3 or 5-node etcd cluster with proper resource allocation

### Resource Requirements

```yaml
# Kubernetes etcd StatefulSet resource requirements
resources:
  requests:
    cpu: "100m"
    memory: "128Mi"
  limits:
    cpu: "500m"
    memory: "1Gi"
```

### Backup Strategy

```bash
# Automated etcd backup script
#!/bin/bash
ETCDCTL_API=3 etcdctl snapshot save /backup/etcd-$(date +%Y%m%d-%H%M%S).db \
  --endpoints=etcd-1:2379,etcd-2:2379,etcd-3:2379 \
  --cacert=/etc/ssl/etcd/ca.pem \
  --cert=/etc/ssl/etcd/client.pem \
  --key=/etc/ssl/etcd/client-key.pem
```

### Monitoring

- **Metrics**: etcd cluster health, operation latency, storage usage
- **Alerts**: Leader changes, high latency, storage approaching limits
- **Dashboards**: etcd-specific Grafana dashboards

## Monitoring & Observability

1. **Metrics**: Expose Prometheus-compatible metrics
2. **Logging**: Structured JSON logging with correlation IDs
3. **Tracing**: Distributed tracing for request flows
4. **Health Checks**: Kubernetes-compatible health endpoints

This architecture provides a solid foundation for the Package Manager while maintaining simplicity and focusing on practical implementation in Rust.
