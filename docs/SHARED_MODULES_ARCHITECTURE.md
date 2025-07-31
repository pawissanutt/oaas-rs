# OaaS Shared Modules - Rust Architecture

## Overview

The OaaS platform uses shared Rust crates to provide common functionality across all components (Package Manager, Class Runtime Manager, CLI tools, etc.). This modular approach ensures consistency, reduces code duplication, and enables independent versioning of shared components.

## Shared Module Structure

```
commons/
├── oprc-grpc/              // gRPC definitions and services
│   ├── Cargo.toml
│   ├── build.rs            // Build script for protobuf compilation
│   ├── proto/              // Protocol buffer definitions
│   │   ├── common.proto    // Common types and enums
│   │   ├── package.proto   // Package management service
│   │   ├── deployment.proto // Deployment service
│   │   ├── runtime.proto   // Runtime state service
│   │   └── health.proto    // Health check service
│   └── src/
│       ├── lib.rs
│       ├── client/         // gRPC client implementations
│       ├── server/         // gRPC server traits and utilities
│       └── types/          // Generated and custom types
├── oprc-models/            // Common data models
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── package.rs      // Package-related models
│       ├── deployment.rs   // Deployment models
│       ├── nfr.rs         // NFR requirement models
│       ├── runtime.rs      // Runtime state models
│       └── validation.rs   // Model validation utilities
├── oprc-storage/           // Storage abstractions
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs       // Storage trait definitions
│       ├── etcd/           // etcd implementation
│       ├── memory/         // In-memory implementation
│       └── error.rs        // Storage error types
├── oprc-observability/     // Observability utilities
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── metrics.rs      // Prometheus metrics utilities
│       ├── tracing.rs      // Structured logging setup
│       ├── health.rs       // Health check utilities
│       └── middleware.rs   // HTTP/gRPC middleware
└── oprc-config/            // Configuration management
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── loader.rs       // Configuration loading utilities
        ├── validation.rs   // Configuration validation
        └── types.rs        // Common configuration types
```

## 1. gRPC Module (oprc-grpc)

### Purpose
Centralized gRPC service definitions and client/server utilities for inter-service communication.

### Protocol Buffer Definitions

#### common.proto
```protobuf
syntax = "proto3";
package oaas.common;

// Common enums and types used across services
enum StatusCode {
  OK = 0;
  ERROR = 1;
  NOT_FOUND = 2;
  INVALID_REQUEST = 3;
  INTERNAL_ERROR = 4;
}

message Timestamp {
  int64 seconds = 1;
  int32 nanos = 2;
}

message ResourceRequirements {
  string cpu_request = 1;
  string memory_request = 2;
  optional string cpu_limit = 3;
  optional string memory_limit = 4;
}

message NfrRequirements {
  optional uint32 max_latency_ms = 1;
  optional uint32 min_throughput_rps = 2;
  optional double availability = 3;
  optional double cpu_utilization_target = 4;
}
```

#### package.proto
```protobuf
syntax = "proto3";
package oaas.package;

import "common.proto";

service PackageService {
  rpc CreatePackage(CreatePackageRequest) returns (CreatePackageResponse);
  rpc GetPackage(GetPackageRequest) returns (GetPackageResponse);
  rpc ListPackages(ListPackagesRequest) returns (ListPackagesResponse);
  rpc DeletePackage(DeletePackageRequest) returns (DeletePackageResponse);
  rpc DeployClass(DeployClassRequest) returns (DeployClassResponse);
}

message OPackage {
  string name = 1;
  optional string version = 2;
  repeated OClass classes = 3;
  repeated OFunction functions = 4;
  repeated string dependencies = 5;
  PackageMetadata metadata = 6;
  bool disabled = 7;
}

message OClass {
  string key = 1;
  string name = 2;
  string package = 3;
  repeated FunctionBinding functions = 4;
  optional StateSpecification state_spec = 5;
  bool disabled = 6;
}

message OFunction {
  string key = 1;
  string name = 2;
  string package = 3;
  string runtime = 4;
  string handler = 5;
  FunctionMetadata metadata = 6;
  bool disabled = 7;
}

message CreatePackageRequest {
  OPackage package = 1;
}

message CreatePackageResponse {
  oaas.common.StatusCode status = 1;
  string package_id = 2;
  optional string message = 3;
}
```

#### deployment.proto
```protobuf
syntax = "proto3";
package oaas.deployment;

import "common.proto";

service DeploymentService {
  rpc Deploy(DeployRequest) returns (DeployResponse);
  rpc GetDeploymentStatus(GetDeploymentStatusRequest) returns (GetDeploymentStatusResponse);
  rpc ScaleDeployment(ScaleDeploymentRequest) returns (ScaleDeploymentResponse);
  rpc DeleteDeployment(DeleteDeploymentRequest) returns (DeleteDeploymentResponse);
}

message OClassDeployment {
  string key = 1;
  string package_name = 2;
  string class_key = 3;
  string target_env = 4;
  repeated string target_clusters = 5;
  oaas.common.NfrRequirements nfr_requirements = 6;
  repeated FunctionDeploymentSpec functions = 7;
  oaas.common.Timestamp created_at = 8;
}

message DeploymentUnit {
  string id = 1;
  string package_name = 2;
  string class_key = 3;
  string target_cluster = 4;
  repeated FunctionDeploymentSpec functions = 5;
  string target_env = 6;
  oaas.common.NfrRequirements nfr_requirements = 7;
  oaas.common.Timestamp created_at = 8;
}

message DeployRequest {
  DeploymentUnit deployment_unit = 1;
}

message DeployResponse {
  oaas.common.StatusCode status = 1;
  string deployment_id = 2;
  optional string message = 3;
}
```

### Rust Client Implementation

**Note**: These gRPC clients are used by all OaaS components, including the Class Runtime Manager for Package Manager communication.

```rust
// src/client/package_client.rs
use tonic::transport::Channel;
use crate::proto::package::{package_service_client::PackageServiceClient, *};

#[derive(Clone)]
pub struct PackageClient {
    client: PackageServiceClient<Channel>,
}

impl PackageClient {
    pub async fn connect(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = PackageServiceClient::connect(endpoint).await?;
        Ok(Self { client })
    }
    
    pub async fn create_package(
        &mut self,
        package: Package,
    ) -> Result<CreatePackageResponse, tonic::Status> {
        let request = tonic::Request::new(CreatePackageRequest {
            package: Some(package),
        });
        
        let response = self.client.create_package(request).await?;
        Ok(response.into_inner())
    }
    
    pub async fn get_package(
        &mut self,
        name: String,
    ) -> Result<GetPackageResponse, tonic::Status> {
        let request = tonic::Request::new(GetPackageRequest { name });
        let response = self.client.get_package(request).await?;
        Ok(response.into_inner())
    }
    
    // Additional methods for CRM integration
    pub async fn report_deployment_status(
        &mut self,
        deployment_id: String,
        status: DeploymentStatus,
    ) -> Result<ReportStatusResponse, tonic::Status> {
        let request = tonic::Request::new(ReportStatusRequest {
            deployment_id,
            status: Some(status),
        });
        let response = self.client.report_deployment_status(request).await?;
        Ok(response.into_inner())
    }
}

// src/client/deployment_client.rs
use tonic::transport::Channel;
use crate::proto::deployment::{deployment_service_client::DeploymentServiceClient, *};

#[derive(Clone)]
pub struct DeploymentClient {
    client: DeploymentServiceClient<Channel>,
}

impl DeploymentClient {
    pub async fn connect(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = DeploymentServiceClient::connect(endpoint).await?;
        Ok(Self { client })
    }
    
    pub async fn deploy(
        &mut self,
        deployment_unit: DeploymentUnit,
    ) -> Result<DeployResponse, tonic::Status> {
        let request = tonic::Request::new(DeployRequest {
            deployment_unit: Some(deployment_unit),
        });
        let response = self.client.deploy(request).await?;
        Ok(response.into_inner())
    }
    
    pub async fn get_deployment_status(
        &mut self,
        deployment_id: String,
    ) -> Result<GetDeploymentStatusResponse, tonic::Status> {
        let request = tonic::Request::new(GetDeploymentStatusRequest { deployment_id });
        let response = self.client.get_deployment_status(request).await?;
        Ok(response.into_inner())
    }
}
}
```

### Server Trait Definitions

```rust
// src/server/package_server.rs
use async_trait::async_trait;
use tonic::{Request, Response, Status};
use crate::proto::package::*;

#[async_trait]
pub trait PackageServiceHandler: Send + Sync + 'static {
    async fn create_package(
        &self,
        request: Request<CreatePackageRequest>,
    ) -> Result<Response<CreatePackageResponse>, Status>;
    
    async fn get_package(
        &self,
        request: Request<GetPackageRequest>,
    ) -> Result<Response<GetPackageResponse>, Status>;
    
    async fn list_packages(
        &self,
        request: Request<ListPackagesRequest>,
    ) -> Result<Response<ListPackagesResponse>, Status>;
    
    async fn delete_package(
        &self,
        request: Request<DeletePackageRequest>,
    ) -> Result<Response<DeletePackageResponse>, Status>;
}

pub struct PackageServiceServer<T: PackageServiceHandler> {
    handler: T,
}

impl<T: PackageServiceHandler> PackageServiceServer<T> {
    pub fn new(handler: T) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl<T: PackageServiceHandler> PackageService for PackageServiceServer<T> {
    async fn create_package(
        &self,
        request: Request<CreatePackageRequest>,
    ) -> Result<Response<CreatePackageResponse>, Status> {
        self.handler.create_package(request).await
    }
    
    // ... other method implementations
}
```

## 2. Models Module (oprc-models)

### Purpose
Shared data models and validation logic used across all services.

### Core Models

```rust
// src/package.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OPackage {
    pub name: String,
    pub version: Option<String>,
    pub classes: Vec<OClass>,
    pub functions: Vec<OFunction>,
    pub dependencies: Vec<String>,
    pub metadata: PackageMetadata,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OClass {
    pub key: String,
    pub name: String,
    pub package: String,
    pub functions: Vec<FunctionBinding>,
    pub state_spec: Option<StateSpecification>,
    pub disabled: bool,
}

// Model validation
impl OPackage {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.is_empty() {
            return Err(ValidationError::EmptyPackageName);
        }
        
        // Validate class keys are unique
        let mut class_keys = std::collections::HashSet::new();
        for class in &self.classes {
            if !class_keys.insert(&class.key) {
                return Err(ValidationError::DuplicateClassKey(class.key.clone()));
            }
        }
        
        // Validate dependencies exist
        for dependency in &self.dependencies {
            if dependency.is_empty() {
                return Err(ValidationError::EmptyDependencyName);
            }
        }
        
        Ok(())
    }
}
```

### Model Conversion Utilities

```rust
// src/conversion.rs
use crate::proto::package as pb;

// No conversion needed since protobuf already defines OPackage, OClass, etc.
// Direct usage of generated types

impl OPackage {
    /// Validation methods for protobuf-generated types
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.is_empty() {
            return Err(ValidationError::EmptyPackageName);
        }
        
        // Validate class keys are unique
        let mut class_keys = std::collections::HashSet::new();
        for class in &self.classes {
            if !class_keys.insert(&class.key) {
                return Err(ValidationError::DuplicateClassKey(class.key.clone()));
            }
        }
        
        // Validate dependencies exist
        for dependency in &self.dependencies {
            if dependency.is_empty() {
                return Err(ValidationError::EmptyDependencyName);
            }
        }
        
        Ok(())
    }
}

impl OClass {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.key.is_empty() {
            return Err(ValidationError::EmptyClassKey);
        }
        
        if self.functions.is_empty() {
            return Err(ValidationError::EmptyFunctionBindings);
        }
        
        Ok(())
    }
}

// Custom serialization helpers if needed for etcd storage
impl OPackage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        // Convert protobuf to JSON for storage
        let json_value = serde_json::to_value(self)?;
        serde_json::to_string(&json_value)
    }
    
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        // Convert JSON back to protobuf type
        serde_json::from_str(json)
    }
}
```

## 3. Storage Module (oprc-storage)

### Purpose
Abstract storage interfaces and implementations for consistent data persistence.

### Storage Traits

```rust
// src/traits.rs
use async_trait::async_trait;
use oprc_models::{OPackage, OClassDeployment, RuntimeState};

#[async_trait]
pub trait PackageStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    
    async fn store_package(&self, package: &OPackage) -> Result<(), Self::Error>;
    async fn get_package(&self, name: &str) -> Result<Option<OPackage>, Self::Error>;
    async fn list_packages(&self, filter: PackageFilter) -> Result<Vec<OPackage>, Self::Error>;
    async fn delete_package(&self, name: &str) -> Result<(), Self::Error>;
}

#[async_trait]
pub trait DeploymentStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    
    async fn store_deployment(&self, deployment: &OClassDeployment) -> Result<(), Self::Error>;
    async fn get_deployment(&self, key: &str) -> Result<Option<OClassDeployment>, Self::Error>;
    async fn list_deployments(&self, filter: DeploymentFilter) -> Result<Vec<OClassDeployment>, Self::Error>;
    async fn delete_deployment(&self, key: &str) -> Result<(), Self::Error>;
}

pub trait StorageFactory {
    type PackageStorage: PackageStorage;
    type DeploymentStorage: DeploymentStorage;
    
    fn create_package_storage(&self) -> Self::PackageStorage;
    fn create_deployment_storage(&self) -> Self::DeploymentStorage;
}
```

### etcd Implementation

```rust
// src/etcd/mod.rs
use async_trait::async_trait;
use etcd_rs::Client;
use crate::traits::*;

pub struct EtcdStorage {
    client: Client,
    key_prefix: String,
}

pub struct EtcdStorageFactory {
    endpoints: Vec<String>,
    key_prefix: String,
    credentials: Option<EtcdCredentials>,
}

impl EtcdStorageFactory {
    pub fn new(endpoints: Vec<String>, key_prefix: String) -> Self {
        Self {
            endpoints,
            key_prefix,
            credentials: None,
        }
    }
    
    pub fn with_credentials(mut self, credentials: EtcdCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }
}

impl StorageFactory for EtcdStorageFactory {
    type PackageStorage = EtcdStorage;
    type DeploymentStorage = EtcdStorage;
    
    fn create_package_storage(&self) -> Self::PackageStorage {
        // Implementation to create etcd client and return EtcdStorage
        todo!()
    }
    
    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        // Implementation to create etcd client and return EtcdStorage
        todo!()
    }
}
```

## 4. Observability Module (oprc-observability)

### Purpose
Standardized observability setup for metrics, logging, and tracing across all services.

### Metrics Utilities

```rust
// src/metrics.rs
use prometheus::{Registry, Counter, Histogram, Gauge};
use std::sync::Arc;

pub struct ServiceMetrics {
    pub requests_total: Counter,
    pub request_duration: Histogram,
    pub active_connections: Gauge,
    pub errors_total: Counter,
}

impl ServiceMetrics {
    pub fn new(service_name: &str, registry: &Registry) -> Result<Self, prometheus::Error> {
        let requests_total = Counter::new(
            format!("{}_requests_total", service_name),
            format!("Total requests processed by {}", service_name)
        )?;
        
        let request_duration = Histogram::new(
            format!("{}_request_duration_seconds", service_name),
            format!("Request duration for {}", service_name)
        )?;
        
        let active_connections = Gauge::new(
            format!("{}_active_connections", service_name),
            format!("Active connections to {}", service_name)
        )?;
        
        let errors_total = Counter::new(
            format!("{}_errors_total", service_name),
            format!("Total errors in {}", service_name)
        )?;
        
        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        
        Ok(ServiceMetrics {
            requests_total,
            request_duration,
            active_connections,
            errors_total,
        })
    }
}

pub fn setup_metrics_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use warp::Filter;
    
    let metrics_route = warp::path("metrics")
        .map(|| {
            use prometheus::{Encoder, TextEncoder};
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            encoder.encode_to_string(&metric_families).unwrap()
        });
    
    let rt = tokio::runtime::Runtime::new()?;
    rt.spawn(async move {
        warp::serve(metrics_route)
            .run(([0, 0, 0, 0], port))
            .await;
    });
    
    Ok(())
}
```

### Tracing Setup

```rust
// src/tracing.rs
use tracing_subscriber::{Layer, Registry, EnvFilter};
use tracing_subscriber::fmt::format::FmtSpan;

pub struct TracingConfig {
    pub service_name: String,
    pub log_level: String,
    pub json_format: bool,
    pub jaeger_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oaas-service".to_string(),
            log_level: "info".to_string(),
            json_format: true,
            jaeger_endpoint: None,
        }
    }
}

pub fn setup_tracing(config: TracingConfig) -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::prelude::*;
    
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true);
    
    let fmt_layer = if config.json_format {
        fmt_layer.json().boxed()
    } else {
        fmt_layer.boxed()
    };
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    
    let mut registry = Registry::default()
        .with(env_filter)
        .with(fmt_layer);
    
    // Add Jaeger tracing if endpoint is provided
    if let Some(jaeger_endpoint) = config.jaeger_endpoint {
        let tracer = opentelemetry_jaeger::new_agent_pipeline()
            .with_service_name(&config.service_name)
            .with_endpoint(&jaeger_endpoint)
            .install_simple()?;
        
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        registry = registry.with(telemetry);
    }
    
    tracing::subscriber::set_global_default(registry)?;
    
    Ok(())
}
```

## 5. Configuration Module (oprc-config)

### Purpose
Centralized configuration loading and validation utilities.

### Configuration Loader

```rust
// src/loader.rs
use serde::de::DeserializeOwned;
use std::path::Path;

pub struct ConfigLoader {
    config_dir: String,
    environment: String,
}

impl ConfigLoader {
    pub fn new(config_dir: String, environment: String) -> Self {
        Self {
            config_dir,
            environment,
        }
    }
    
    pub fn load<T: DeserializeOwned>(&self) -> Result<T, ConfigError> {
        let default_path = format!("{}/default.yaml", self.config_dir);
        let env_path = format!("{}/{}.yaml", self.config_dir, self.environment);
        
        let mut config = config::Config::builder();
        
        // Load default configuration
        if Path::new(&default_path).exists() {
            config = config.add_source(config::File::with_name(&default_path));
        }
        
        // Override with environment-specific configuration
        if Path::new(&env_path).exists() {
            config = config.add_source(config::File::with_name(&env_path));
        }
        
        // Override with environment variables
        config = config.add_source(
            config::Environment::with_prefix("OAAS")
                .prefix_separator("_")
                .separator("__")
        );
        
        let config = config.build()?;
        let parsed_config: T = config.try_deserialize()?;
        
        Ok(parsed_config)
    }
}
```

### Common Configuration Types

```rust
// src/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: Option<usize>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub cert_file: String,
    pub key_file: String,
    pub ca_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub json_format: bool,
    pub metrics_port: u16,
    pub jaeger_endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
    pub key_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: Option<TlsConfig>,
    pub timeout_seconds: Option<u64>,
}
```

## Usage Examples

### In Package Manager

```rust
// Package Manager Cargo.toml
[dependencies]
oprc-grpc = { path = "../commons/oprc-grpc" }
oprc-models = { path = "../commons/oprc-models" }
oprc-storage = { path = "../commons/oprc-storage" }
oprc-observability = { path = "../commons/oprc-observability" }
oprc-config = { path = "../commons/oprc-config" }

// Package Manager main.rs
use oprc_config::ConfigLoader;
use oprc_observability::{setup_tracing, TracingConfig};
use oprc_storage::StorageFactory;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_loader = ConfigLoader::new("config".to_string(), "production".to_string());
    let config: PackageManagerConfig = config_loader.load()?;
    
    // Setup observability
    setup_tracing(TracingConfig {
        service_name: "package-manager".to_string(),
        log_level: config.observability.log_level.clone(),
        json_format: config.observability.json_format,
        jaeger_endpoint: config.observability.jaeger_endpoint.clone(),
    })?;
    
    // Create storage
    let storage_factory = EtcdStorageFactory::new(
        config.storage.etcd.endpoints.clone(),
        config.storage.etcd.key_prefix.clone(),
    );
    let package_storage = storage_factory.create_package_storage();
    
    // Start server
    start_server(config, package_storage).await
}
```

### In Class Runtime Manager

```rust
// CRM Cargo.toml - same dependencies
use oprc_grpc::client::PackageClient;
use oprc_models::DeploymentUnit;

async fn deploy_from_package_manager() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = PackageClient::connect("http://package-manager:50051".to_string()).await?;
    
    let package = client.get_package("my-package".to_string()).await?;
    
    // Process deployment using shared models
    let deployment_unit = DeploymentUnit {
        id: "deploy-123".to_string(),
        package_name: package.name,
        // ... other fields
    };
    
    Ok(())
}
```

## Benefits of Shared Modules

1. **Consistency**: Same data models and validation across all services
2. **Maintainability**: Single source of truth for common functionality
3. **Type Safety**: Compile-time guarantees for inter-service communication
4. **Versioning**: Independent versioning of shared components
5. **Testing**: Shared test utilities and fixtures
6. **Performance**: Compiled gRPC code with optimal serialization

This shared module architecture provides a solid foundation for building scalable, maintainable OaaS services in Rust.
