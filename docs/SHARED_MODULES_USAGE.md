# OaaS Shared Modules Usage Examples

This document demonstrates how to use the shared modules in your OaaS services.

## Package Manager Service Example

```rust
// Cargo.toml
[dependencies]
oprc-grpc = { path = "../commons/oprc-grpc" }
oprc-models = { path = "../commons/oprc-models" }
oprc-storage = { path = "../commons/oprc-storage" }
oprc-observability = { path = "../commons/oprc-observability" }
oprc-config = { path = "../commons/oprc-config" }

// main.rs
use oprc_config::{ConfigLoader, ServiceConfig, Validate};
use oprc_observability::{setup_tracing, TracingConfig, ServiceMetrics};
use oprc_storage::{StorageFactory, memory::MemoryStorageFactory};
use oprc_grpc::server::{PackageServiceHandler, PackageServiceServer};
use prometheus::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_loader = ConfigLoader::from_env()?;
    let config: ServiceConfig = config_loader.load()?;
    config.validate()?;
    
    // Setup observability
    setup_tracing(TracingConfig {
        service_name: config.name.clone(),
        log_level: config.observability.log_level.clone(),
        json_format: config.observability.json_format,
        jaeger_endpoint: config.observability.jaeger_endpoint.clone(),
    })?;
    
    // Setup metrics
    let registry = Registry::new();
    let metrics = ServiceMetrics::new(&config.name, &registry)?;
    
    // Create storage
    let storage_factory = MemoryStorageFactory;
    let package_storage = storage_factory.create_package_storage();
    
    // Create service handler
    let handler = MyPackageHandler::new(package_storage);
    let service = PackageServiceServer::new(handler);
    
    // Start server
    tonic::transport::Server::builder()
        .add_service(service)
        .serve(format!("{}:{}", config.server.host, config.server.port).parse()?)
        .await?;
    
    Ok(())
}
```

## Class Runtime Manager Example

```rust
// Cargo.toml - same dependencies
use oprc_grpc::client::PackageClient;
use oprc_models::{DeploymentUnit, OClassDeployment};

async fn deploy_from_package_manager() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Package Manager
    let mut client = PackageClient::connect("http://package-manager:50051".to_string()).await?;
    
    // Get package information
    let package_response = client.get_package("my-package".to_string()).await?;
    
    if let Some(package) = package_response.package {
        // Create deployment using shared models
        let deployment_unit = DeploymentUnit {
            id: uuid::Uuid::new_v4().to_string(),
            package_name: package.name,
            class_key: "my-class".to_string(),
            target_cluster: "prod-cluster".to_string(),
            functions: vec![], // Fill from package.classes
            target_env: "production".to_string(),
            nfr_requirements: Default::default(),
            created_at: chrono::Utc::now(),
        };
        
        // Deploy to cluster (implementation specific)
        deploy_to_cluster(deployment_unit).await?;
    }
    
    Ok(())
}
```

## Configuration Files

Create these configuration files in your service's `config/` directory:

### config/default.yaml
```yaml
name: "package-manager"
server:
  host: "0.0.0.0"
  port: 50051
  workers: 4

observability:
  log_level: "info"
  json_format: true
  metrics_port: 9090

storage:
  type: "memory"
```

### config/production.yaml
```yaml
server:
  host: "0.0.0.0"
  port: 50051

observability:
  log_level: "warn"
  jaeger_endpoint: "http://jaeger:14268/api/traces"

storage:
  type: "etcd"
  etcd:
    endpoints:
      - "http://etcd-1:2379"
      - "http://etcd-2:2379"
      - "http://etcd-3:2379"
    key_prefix: "/oaas/prod"
    username: "oaas"
    password: "${ETCD_PASSWORD}"
    timeout_seconds: 30
```

## Environment Variables

Set these environment variables:

```bash
# Configuration
export OAAS_ENV=production
export OAAS_CONFIG_DIR=./config

# Service-specific overrides
export OAAS_SERVER__PORT=8080
export OAAS_OBSERVABILITY__LOG_LEVEL=debug
export OAAS_STORAGE__ETCD__ENDPOINTS="http://etcd:2379"

# Secrets
export ETCD_PASSWORD=your-etcd-password
```

## Building and Running

```bash
# Build all shared modules
cargo build --workspace

# Run with specific environment
OAAS_ENV=production cargo run --bin package-manager

# Run tests
cargo test --workspace
```
