# Package Manager - Rust Architecture & Implementation Plan

## Overview

The Package Manager serves as the central control plane for the OaaS (Object-as-a-Service) platform, written in Rust for performance, safety, and reliability. This document outlines a pragmatic architecture focusing on MVP implementation while maintaining extensibility.

## Architectural Improvements & Simplifications

### Key Changes from Original Specification

1. **Distributed Storage**: Use etcd for distributed, consistent storage with built-in clustering
2. **Async-First Design**: Leverage Rust's async ecosystem (tokio, async-std)
3. **Type Safety**: Use Rust's type system to prevent runtime errors in deployment logic
4. **Modular Architecture**: Clear separation of concerns with well-defined interfaces
5. **Configuration-Driven**: YAML/TOML-based configuration with environment overrides
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

**Note**: The Package Manager should be organized within a `control-plane` directory alongside the Class Runtime Manager to clearly separate control plane components from data plane components (like ODGM). The recommended workspace structure is:

```
control-plane/
├── oprc-pm/                     // Package Manager (this component)
└── oprc-crm/                    // Class Runtime Manager
```

```
oprc-pm/
├── src/
│   ├── main.rs              // Application entry point
│   ├── lib.rs               // Library exports
│   ├── config/              // Configuration management
│   │   ├── mod.rs
│   │   ├── app.rs           // Application config
│   │   └── environment.rs   // Environment-specific config
│   ├── models/              // Data models and schemas
│   │   ├── mod.rs
│   │   ├── package.rs       // OPackage, OClass, OFunction
│   │   ├── deployment.rs    // Deployment-related models
│   │   ├── runtime.rs       // Runtime state models
│   │   └── errors.rs        // Error types
│   ├── storage/             // Data persistence layer
│   │   ├── mod.rs
│   │   ├── etcd.rs          // etcd distributed storage
│   │   ├── memory.rs        // In-memory storage for testing
│   │   └── traits.rs        // Storage abstractions
│   ├── services/            // Business logic services
│   │   ├── mod.rs
│   │   ├── package.rs       // Package management logic
│   │   ├── deployment.rs    // Deployment orchestration
│   │   ├── runtime_state.rs // Runtime state management
│   │   └── validation.rs    // Package validation
│   ├── api/                 // HTTP API layer
│   │   ├── mod.rs
│   │   ├── handlers/        // HTTP handlers
│   │   ├── middleware/      // Request middleware
│   │   └── extractors.rs    // Request extractors
│   ├── crm/                 // CRM integration
│   │   ├── mod.rs
│   │   ├── client.rs        // HTTP client for CRM communication
│   │   └── events.rs        // Event publishing
│   └── observability/       // Logging, metrics, tracing
│       ├── mod.rs
│       ├── metrics.rs
│       └── tracing.rs
├── tests/
│   ├── integration/
│   └── fixtures/
├── config/
│   ├── default.yaml
│   ├── development.yaml
│   └── production.yaml
├── scripts/
│   ├── generate-crds.sh         // CRD generation automation
│   ├── validate-crds.sh         // CRD validation script  
│   └── deploy-crds.sh           // CRD deployment script
├── k8s/
│   ├── crds/                    // Generated CRD manifests
│   │   ├── oclassdeployments.yaml
│   │   ├── oruntimestates.yaml
│   │   └── opackages.yaml
│   └── manifests/               // Kubernetes deployment manifests
└── xtask/                       // Cargo xtask automation
    └── src/
        └── main.rs              // Build automation tasks
```

### 2. Data Models (Using gRPC Generated Types)

The Package Manager uses gRPC generated types from the shared `oprc-grpc` module instead of defining its own models. This ensures consistency across services and eliminates model duplication.

#### Primary Data Types

```rust
// Directly use gRPC generated types (with O prefix defined in protobuf)
pub use oprc_grpc::proto::package::{
    OPackage,
    OClass, 
    OFunction,
    PackageMetadata,
    FunctionBinding,
    StateSpecification,
    // ... other package-related types
};

pub use oprc_grpc::proto::deployment::{
    DeploymentUnit,
    OClassDeployment,
    ResourceRequirements,
    NfrRequirements,
    // ... other deployment-related types
};

pub use oprc_grpc::proto::common::{
    ResourceAllocation,
    Timestamp,
    StatusCode,
    // ... other common types
};
```

#### Extended Types for Package Manager Specific Logic

```rust
// Additional types specific to Package Manager that extend gRPC types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentReplicas {
    pub min: u32,
    pub max: u32,
    pub target: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub partition_key: String,
    pub partition_count: u32,
    pub replication_factor: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub key: String,
    pub class_key: String,
    pub deployment_id: String,
    pub status: RuntimeStatus,
    pub health: HealthStatus,
    pub metrics: RuntimeMetrics,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub request_count: u64,
    pub error_count: u64,
    pub average_latency: f64,
}

// Filter types for queries (not in gRPC as they're API-specific)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageFilter {
    pub name_pattern: Option<String>,
    pub disabled: Option<bool>,
    pub tags: Vec<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl PackageFilter {
    pub fn matches(&self, package: &OPackage) -> bool {
        if let Some(disabled) = self.disabled {
            if package.disabled != disabled {
                return false;
            }
        }
        
        if let Some(ref pattern) = self.name_pattern {
            if !package.name.contains(pattern) {
                return false;
            }
        }
        
        if !self.tags.is_empty() {
            let package_tags: HashSet<String> = package.metadata
                .as_ref()
                .map(|m| m.tags.iter().cloned().collect())
                .unwrap_or_default();
            let filter_tags: HashSet<String> = self.tags.iter().cloned().collect();
            if filter_tags.intersection(&package_tags).count() == 0 {
                return false;
            }
        }
        
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeploymentFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub target_env: Option<String>,
    pub status: Option<i32>, // Use proto enum values
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// Conversion utilities between gRPC types and internal representations
impl From<oprc_grpc::proto::package::Package> for OPackage {
    fn from(proto: oprc_grpc::proto::package::Package) -> Self {
        proto // Direct use since they're the same type
    }
}

impl From<OPackage> for oprc_grpc::proto::package::Package {
    fn from(package: OPackage) -> Self {
        package // Direct use since they're the same type
    }
}
```

### 3. Storage Layer (Using gRPC Types)

#### Storage Abstraction

```rust
use oprc_grpc::proto::package::Package as OPackage;
use oprc_grpc::proto::deployment::DeploymentRequest as OClassDeployment;

#[async_trait]
pub trait PackageStorage: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    
    // Package operations (using gRPC generated types)
    async fn store_package(&self, package: &OPackage) -> Result<(), Self::Error>;
    async fn get_package(&self, name: &str) -> Result<Option<OPackage>, Self::Error>;
    async fn list_packages(&self, filter: PackageFilter) -> Result<Vec<OPackage>, Self::Error>;
    async fn delete_package(&self, name: &str) -> Result<(), Self::Error>;
    
    // Deployment operations (using gRPC generated types)
    async fn store_deployment(&self, deployment: &OClassDeployment) -> Result<(), Self::Error>;
    async fn get_deployment(&self, key: &str) -> Result<Option<OClassDeployment>, Self::Error>;
    async fn list_deployments(&self, filter: DeploymentFilter) -> Result<Vec<OClassDeployment>, Self::Error>;
    
    // Runtime state operations
    async fn update_runtime_state(&self, state: &RuntimeState) -> Result<(), Self::Error>;
    async fn get_runtime_state(&self, key: &str) -> Result<Option<RuntimeState>, Self::Error>;
}
```

#### etcd Implementation

```rust
// Using etcd-rs for distributed storage
pub struct EtcdStorage {
    client: etcd_rs::Client,
    key_prefix: String,
}

impl EtcdStorage {
    pub async fn new(endpoints: Vec<String>, key_prefix: String) -> Result<Self, etcd_rs::Error> {
        let client = etcd_rs::Client::connect(endpoints, None).await?;
        
        Ok(Self {
            client,
            key_prefix,
        })
    }
    
    fn package_key(&self, name: &str) -> String {
        format!("{}/packages/{}", self.key_prefix, name)
    }
    
    fn deployment_key(&self, key: &str) -> String {
        format!("{}/deployments/{}", self.key_prefix, key)
    }
    
    fn runtime_state_key(&self, key: &str) -> String {
        format!("{}/runtime_states/{}", self.key_prefix, key)
    }
}

#[async_trait]
impl PackageStorage for EtcdStorage {
    type Error = EtcdStorageError;
    
    async fn store_package(&self, package: &OPackage) -> Result<(), Self::Error> {
        let key = self.package_key(&package.name);
        let value = serde_json::to_string(package)?;
        
        self.client.kv().put(key, value, None).await?;
        Ok(())
    }
    
    async fn get_package(&self, name: &str) -> Result<Option<OPackage>, Self::Error> {
        let key = self.package_key(name);
        let response = self.client.kv().get(key, None).await?;
        
        if response.kvs().is_empty() {
            return Ok(None);
        }
        
        let value = response.kvs()[0].value_str()?;
        let package: OPackage = serde_json::from_str(value)?;
        Ok(Some(package))
    }
    
    async fn list_packages(&self, filter: PackageFilter) -> Result<Vec<OPackage>, Self::Error> {
        let prefix = format!("{}/packages/", self.key_prefix);
        let response = self.client.kv().get_prefix(prefix, None).await?;
        
        let mut packages = Vec::new();
        for kv in response.kvs() {
            let value = kv.value_str()?;
            let package: OPackage = serde_json::from_str(value)?;
            
            if filter.matches(&package) {
                packages.push(package);
            }
        }
        
        Ok(packages)
    }
    
    async fn delete_package(&self, name: &str) -> Result<(), Self::Error> {
        let key = self.package_key(name);
        self.client.kv().delete(key, None).await?;
        Ok(())
    }
    
    async fn store_deployment(&self, deployment: &OClassDeployment) -> Result<(), Self::Error> {
        let key = self.deployment_key(&deployment.key);
        let value = serde_json::to_string(deployment)?;
        
        self.client.kv().put(key, value, None).await?;
        Ok(())
    }
    
    async fn get_deployment(&self, key: &str) -> Result<Option<OClassDeployment>, Self::Error> {
        let etcd_key = self.deployment_key(key);
        let response = self.client.kv().get(etcd_key, None).await?;
        
        if response.kvs().is_empty() {
            return Ok(None);
        }
        
        let value = response.kvs()[0].value_str()?;
        let deployment: OClassDeployment = serde_json::from_str(value)?;
        Ok(Some(deployment))
    }
    
    async fn list_deployments(&self, filter: DeploymentFilter) -> Result<Vec<OClassDeployment>, Self::Error> {
        let prefix = format!("{}/deployments/", self.key_prefix);
        let response = self.client.kv().get_prefix(prefix, None).await?;
        
        let mut deployments = Vec::new();
        for kv in response.kvs() {
            let value = kv.value_str()?;
            let deployment: OClassDeployment = serde_json::from_str(value)?;
            
            if filter.matches(&deployment) {
                deployments.push(deployment);
            }
        }
        
        Ok(deployments)
    }
    
    async fn update_runtime_state(&self, state: &RuntimeState) -> Result<(), Self::Error> {
        let key = self.runtime_state_key(&state.key);
        let value = serde_json::to_string(state)?;
        
        self.client.kv().put(key, value, None).await?;
        Ok(())
    }
    
    async fn get_runtime_state(&self, key: &str) -> Result<Option<RuntimeState>, Self::Error> {
        let etcd_key = self.runtime_state_key(key);
        let response = self.client.kv().get(etcd_key, None).await?;
        
        if response.kvs().is_empty() {
            return Ok(None);
        }
        
        let value = response.kvs()[0].value_str()?;
        let state: RuntimeState = serde_json::from_str(value)?;
        Ok(Some(state))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EtcdStorageError {
    #[error("etcd client error: {0}")]
    EtcdError(#[from] etcd_rs::Error),
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("utf8 conversion error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}
```

#### Storage Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub storage_type: StorageType,
    pub etcd: Option<EtcdConfig>,
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
    pub key_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: Option<EtcdTlsConfig>,
    pub timeout: Option<u64>, // seconds
}

#[derive(Debug, Clone, Deserialize)]
pub struct EtcdTlsConfig {
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub insecure: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub enum StorageType {
    Etcd,
    Memory,
}
```

### 4. Service Layer

#### Package Service

```rust
pub struct PackageService {
    storage: Arc<dyn PackageStorage>,
    deployment_service: Arc<DeploymentService>,
    validator: PackageValidator,
}

impl PackageService {
    pub async fn create_package(&self, package: OPackage) -> Result<PackageId, PackageError> {
        // 1. Validate package structure
        self.validator.validate(&package)?;
        
        // 2. Check dependencies
        self.validate_dependencies(&package.dependencies).await?;
        
        // 3. Store package
        self.storage.store_package(&package).await?;
        
        // 4. Trigger deployments if configured
        self.deployment_service.schedule_deployments(&package).await?;
        
        Ok(PackageId::new(package.name))
    }
    
    pub async fn update_package(&self, package: OPackage) -> Result<(), PackageError> {
        // Handle package updates with migration logic
        todo!()
    }
    
    pub async fn delete_package(&self, name: &str) -> Result<(), PackageError> {
        // 1. Check for dependent packages
        // 2. Cleanup deployments
        // 3. Remove package
        todo!()
    }
}
```

#### Deployment Service

```rust
pub struct DeploymentService {
    storage: Arc<dyn PackageStorage>,
    crm_manager: Arc<CrmManager>, // Changed from single CrmClient to CrmManager
    event_publisher: Arc<EventPublisher>,
}

impl DeploymentService {
    pub async fn deploy_class(
        &self,
        class: &OClass,
        deployment: &OClassDeployment,
    ) -> Result<DeploymentId, DeploymentError> {
        // 1. Calculate deployment requirements
        let requirements = self.calculate_requirements(class, deployment).await?;
        
        // 2. Create deployment units for each environment and cluster combination
        let units = self.create_deployment_units(class, deployment, requirements).await?;
        
        // 3. Send to appropriate CRM instances based on target cluster
        for unit in units {
            let crm_client = self.crm_manager.get_client(&unit.target_cluster).await?;
            crm_client.deploy(unit).await?;
        }
        
        // 4. Update deployment state
        self.storage.store_deployment(deployment).await?;
        
        Ok(DeploymentId::new())
    }
    
    async fn calculate_requirements(
        &self,
        class: &OClass,
        deployment: &OClassDeployment,
    ) -> Result<DeploymentRequirements, DeploymentError> {
        // Calculate replica counts based on availability requirements
        // Determine resource allocations
        // Validate environment constraints
        todo!()
    }
}
```

### 5. API Layer

#### API Endpoints Overview

The Package Manager exposes RESTful APIs for managing packages, deployments, and tracking deployment records from the Class Runtime Manager:

**Package Management APIs:**
- `POST /api/v1/packages` - Create a new package
- `GET /api/v1/packages` - List packages with optional filtering
- `GET /api/v1/packages/{name}` - Get specific package details
- `DELETE /api/v1/packages/{name}` - Delete a package

**Deployment Management APIs:**
- `POST /api/v1/deployments` - Create a new deployment
- `GET /api/v1/deployments` - List deployments with filtering

**Deployment Record APIs (from CRM clusters):**
- `GET /api/v1/deployment-records` - List deployment records from all CRM clusters
- `GET /api/v1/deployment-records?cluster=prod-east` - List records from specific cluster
- `GET /api/v1/deployment-records/{id}` - Get specific deployment record (searches all clusters)
- `GET /api/v1/deployment-records/{id}?cluster=prod-east` - Get record from specific cluster
- `GET /api/v1/deployment-status/{id}` - Get deployment status (uses default cluster)
- `GET /api/v1/deployment-status/{id}?cluster=prod-east` - Get status from specific cluster
- `DELETE /api/v1/deployments/{id}?cluster=prod-east` - Delete deployment (cluster required)

**Multi-cluster Management APIs:**
- `GET /api/v1/clusters` - List all configured clusters with health status
- `GET /api/v1/clusters/{name}/health` - Get health status of specific cluster

**Query APIs:**
- `GET /api/v1/classes` - List classes across packages
- `GET /api/v1/functions` - List functions across packages

#### HTTP Server Setup

```rust
// Using axum for the HTTP server
pub struct ApiServer {
    app: Router,
    config: ApiConfig,
}

impl ApiServer {
    pub fn new(
        package_service: Arc<PackageService>,
        deployment_service: Arc<DeploymentService>,
        crm_manager: Arc<CrmManager>, // Changed from crm_client to crm_manager
        config: ApiConfig,
    ) -> Self {
        let app = Router::new()
            // Package Management APIs
            .route("/api/v1/packages", post(handlers::create_package))
            .route("/api/v1/packages", get(handlers::list_packages))
            .route("/api/v1/packages/:name", get(handlers::get_package))
            .route("/api/v1/packages/:name", delete(handlers::delete_package))
            
            // Deployment Management APIs
            .route("/api/v1/deployments", get(handlers::list_deployments))
            .route("/api/v1/deployments", post(handlers::create_deployment))
            
            // Deployment Record APIs (from CRM clusters)
            .route("/api/v1/deployment-records", get(handlers::list_deployment_records))
            .route("/api/v1/deployment-records/:id", get(handlers::get_deployment_record))
            .route("/api/v1/deployment-status/:id", get(handlers::get_deployment_status))
            .route("/api/v1/deployments/:id", delete(handlers::delete_deployment))
            
            // Multi-cluster Management APIs
            .route("/api/v1/clusters", get(handlers::list_clusters))
            .route("/api/v1/clusters/:name/health", get(handlers::get_cluster_health))
            
            // Class and Function APIs
            .route("/api/v1/classes", get(handlers::list_classes))
            .route("/api/v1/functions", get(handlers::list_functions))
            
            .layer(middleware::logging())
            .layer(middleware::metrics())
            .with_state(AppState {
                package_service,
                deployment_service,
                crm_manager, // Updated field name
            });
            
        Self { app, config }
    }
    
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        
        tracing::info!("Package Manager listening on {}", addr);
        axum::serve(listener, self.app).await?;
        
        Ok(())
    }
}
```

#### Application State

```rust
#[derive(Clone)]
pub struct AppState {
    pub package_service: Arc<PackageService>,
    pub deployment_service: Arc<DeploymentService>,
    pub crm_manager: Arc<CrmManager>, // Changed from crm_client
}
```

#### API Handlers

```rust
pub async fn create_package(
    State(state): State<AppState>,
    Json(package): Json<OPackage>,
) -> Result<Json<PackageResponse>, ApiError> {
    let package_id = state.package_service.create_package(package).await?;
    
    Ok(Json(PackageResponse {
        id: package_id.to_string(),
        status: "created".to_string(),
    }))
}

pub async fn list_packages(
    State(state): State<AppState>,
    Query(filter): Query<PackageFilter>,
) -> Result<Json<Vec<OPackage>>, ApiError> {
    let packages = state.package_service.list_packages(filter).await?;
    Ok(Json(packages))
}

// Deployment Record API Handlers
pub async fn get_deployment_record(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<DeploymentRecord>, ApiError> {
    // If cluster is specified, query that specific cluster
    if let Some(cluster_name) = params.get("cluster") {
        let client = state.crm_manager.get_client(cluster_name).await?;
        let record = client.get_deployment_record(&id).await?;
        Ok(Json(record))
    } else {
        // Search across all clusters
        let filter = DeploymentRecordFilter {
            limit: Some(1),
            ..Default::default()
        };
        let records = state.crm_manager.get_all_deployment_records(filter).await?;
        let record = records.into_iter()
            .find(|r| r.id == id)
            .ok_or(ApiError::NotFound("Deployment record not found".to_string()))?;
        Ok(Json(record))
    }
}

pub async fn list_deployment_records(
    State(state): State<AppState>,
    Query(filter): Query<DeploymentRecordFilter>,
) -> Result<Json<Vec<DeploymentRecord>>, ApiError> {
    let records = state.crm_manager.get_all_deployment_records(filter).await?;
    Ok(Json(records))
}

pub async fn get_deployment_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<DeploymentStatus>, ApiError> {
    // Try to get status from specific cluster if provided
    if let Some(cluster_name) = params.get("cluster") {
        let client = state.crm_manager.get_client(cluster_name).await?;
        let status = client.get_deployment_status(&id).await?;
        Ok(Json(status))
    } else {
        // Use default cluster or search across clusters
        let client = state.crm_manager.get_default_client().await?;
        let status = client.get_deployment_status(&id).await?;
        Ok(Json(status))
    }
}

pub async fn delete_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(cluster_name) = params.get("cluster") {
        let client = state.crm_manager.get_client(cluster_name).await?;
        client.delete_deployment(&id).await?;
        Ok(Json(serde_json::json!({
            "message": "Deployment deleted successfully",
            "id": id,
            "cluster": cluster_name
        })))
    } else {
        return Err(ApiError::BadRequest("cluster parameter is required for deployment deletion".to_string()));
    }
}

// Multi-cluster Management API Handlers
pub async fn list_clusters(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClusterInfo>>, ApiError> {
    let cluster_names = state.crm_manager.list_clusters().await;
    let mut cluster_infos = Vec::new();
    
    for cluster_name in cluster_names {
        let health = match state.crm_manager.get_cluster_health(&cluster_name).await {
            Ok(health) => health,
            Err(_) => ClusterHealth {
                cluster_name: cluster_name.clone(),
                status: "Unknown".to_string(),
                crm_version: None,
                last_seen: Utc::now(),
                node_count: None,
                ready_nodes: None,
            },
        };
        
        cluster_infos.push(ClusterInfo {
            name: cluster_name,
            health,
        });
    }
    
    Ok(Json(cluster_infos))
}

pub async fn get_cluster_health(
    State(state): State<AppState>,
    Path(cluster_name): Path<String>,
) -> Result<Json<ClusterHealth>, ApiError> {
    let health = state.crm_manager.get_cluster_health(&cluster_name).await?;
    Ok(Json(health))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub name: String,
    pub health: ClusterHealth,
}
```

### 6. CRM Integration

#### Multi-Cluster CRM Architecture

The Package Manager supports connecting to multiple Class Runtime Manager (CRM) instances, typically one per Kubernetes cluster. This enables multi-cluster deployments where each cluster has its own CRM managing local resources.

#### CRM Manager

```rust
pub struct CrmManager {
    clients: HashMap<String, Arc<CrmClient>>, // cluster_name -> CrmClient
    default_client: Option<Arc<CrmClient>>,
    config: CrmManagerConfig,
}

impl CrmManager {
    pub fn new(config: CrmManagerConfig) -> Result<Self, CrmError> {
        let mut clients = HashMap::new();
        let mut default_client = None;
        
        for (cluster_name, crm_config) in config.clusters {
            let client = Arc::new(CrmClient::new(crm_config)?);
            
            if config.default_cluster.as_ref() == Some(&cluster_name) {
                default_client = Some(client.clone());
            }
            
            clients.insert(cluster_name, client);
        }
        
        Ok(Self {
            clients,
            default_client,
            config,
        })
    }
    
    pub async fn get_client(&self, cluster_name: &str) -> Result<Arc<CrmClient>, CrmError> {
        self.clients
            .get(cluster_name)
            .cloned()
            .ok_or_else(|| CrmError::ClusterNotFound(cluster_name.to_string()))
    }
    
    pub async fn get_default_client(&self) -> Result<Arc<CrmClient>, CrmError> {
        self.default_client
            .clone()
            .ok_or(CrmError::NoDefaultCluster)
    }
    
    pub async fn list_clusters(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }
    
    pub async fn get_cluster_health(&self, cluster_name: &str) -> Result<ClusterHealth, CrmError> {
        let client = self.get_client(cluster_name).await?;
        client.health_check().await
    }
    
    pub async fn get_all_deployment_records(&self, filter: DeploymentRecordFilter) -> Result<Vec<DeploymentRecord>, CrmError> {
        let mut all_records = Vec::new();
        
        // Query all clusters or specific cluster if specified in filter
        let clusters_to_query = if let Some(cluster) = &filter.cluster {
            vec![cluster.clone()]
        } else {
            self.list_clusters().await
        };
        
        for cluster_name in clusters_to_query {
            if let Ok(client) = self.get_client(&cluster_name).await {
                match client.list_deployment_records(filter.clone()).await {
                    Ok(mut records) => {
                        // Tag records with their cluster information
                        for record in &mut records {
                            record.cluster_name = Some(cluster_name.clone());
                        }
                        all_records.extend(records);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch records from cluster {}: {}", cluster_name, e);
                        // Continue with other clusters
                    }
                }
            }
        }
        
        // Apply global filtering and pagination
        if let Some(limit) = filter.limit {
            all_records.truncate(limit);
        }
        
        Ok(all_records)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub status: String, // Healthy, Degraded, Unhealthy
    pub crm_version: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub node_count: Option<u32>,
    pub ready_nodes: Option<u32>,
}
```

#### Implementation Notes

Add the following imports for multi-cluster CRM support:

```rust
use std::collections::HashMap;
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing;
```

#### CRM Client (Per Cluster)

```rust
pub struct CrmClient {
    client: reqwest::Client,
    base_url: String,
    cluster_name: String,
    config: CrmClientConfig,
}

impl CrmClient {
    pub fn new(config: CrmClientConfig) -> Result<Self, CrmError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()?;
            
        Ok(Self {
            client,
            base_url: config.base_url.clone(),
            cluster_name: config.cluster_name.clone(),
            config,
        })
    }
    
    pub async fn health_check(&self) -> Result<ClusterHealth, CrmError> {
        let response = self
            .client
            .get(&format!("{}/api/v1/health", self.base_url))
            .send()
            .await?;
            
        let health: ClusterHealth = response.json().await?;
        Ok(health)
    }
    pub async fn deploy(&self, unit: DeploymentUnit) -> Result<DeploymentResponse, CrmError> {
        let response = self
            .client
            .post(&format!("{}/api/v1/deployments", self.base_url))
            .json(&unit)
            .send()
            .await?;
            
        let deployment_response: DeploymentResponse = response.json().await?;
        Ok(deployment_response)
    }
    
    pub async fn get_deployment_status(&self, id: &str) -> Result<DeploymentStatus, CrmError> {
        let response = self
            .client
            .get(&format!("{}/api/v1/deployments/{}", self.base_url, id))
            .send()
            .await?;
            
        let status: DeploymentStatus = response.json().await?;
        Ok(status)
    }
    
    pub async fn get_deployment_record(&self, id: &str) -> Result<DeploymentRecord, CrmError> {
        let response = self
            .client
            .get(&format!("{}/api/v1/deployment-records/{}", self.base_url, id))
            .send()
            .await?;
        
        let record: DeploymentRecord = response.json().await?;
        Ok(record)
    }
    
    pub async fn list_deployment_records(&self, filter: DeploymentRecordFilter) -> Result<Vec<DeploymentRecord>, CrmError> {
        let mut url = format!("{}/api/v1/deployment-records", self.base_url);
        let mut query_params = Vec::new();
        
        if let Some(package_name) = &filter.package_name {
            query_params.push(format!("package={}", package_name));
        }
        if let Some(class_key) = &filter.class_key {
            query_params.push(format!("class={}", class_key));
        }
        if let Some(environment) = &filter.environment {
            query_params.push(format!("env={}", environment));
        }
        if let Some(status) = &filter.status {
            query_params.push(format!("status={}", status));
        }
        if let Some(limit) = filter.limit {
            query_params.push(format!("limit={}", limit));
        }
        if let Some(offset) = filter.offset {
            query_params.push(format!("offset={}", offset));
        }
        
        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }
        
        let response = self
            .client
            .get(&url)
            .send()
            .await?;
        
        let records: Vec<DeploymentRecord> = response.json().await?;
        Ok(records)
    }
    
    pub async fn delete_deployment(&self, id: &str) -> Result<(), CrmError> {
        let response = self
            .client
            .delete(&format!("{}/api/v1/deployments/{}", self.base_url, id))
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(CrmError::RequestFailed(response.status()));
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeploymentRecordFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub environment: Option<String>,
    pub cluster: Option<String>, // Filter by specific cluster
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum CrmError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Request failed with status: {0}")]
    RequestFailed(reqwest::StatusCode),
    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("CRM service unavailable")]
    ServiceUnavailable,
    #[error("Cluster not found: {0}")]
    ClusterNotFound(String),
    #[error("No default cluster configured")]
    NoDefaultCluster,
    #[error("Client configuration error: {0}")]
    ConfigurationError(String),
}

// CRM API Response Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: String,
    pub deployment_unit_id: String,
    pub package_name: String,
    pub class_key: String,
    pub target_environment: String,
    pub cluster_name: Option<String>, // Which cluster this record is from
    pub status: DeploymentRecordStatus,
    pub nfr_compliance: Option<NfrCompliance>,
    pub resource_refs: Vec<ResourceReference>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecordStatus {
    pub condition: String, // Pending, Provisioning, Running, Scaling, Failed, Terminated
    pub phase: String,     // TemplateSelection, ResourceProvisioning, etc.
    pub message: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfrCompliance {
    pub overall_status: String, // Compliant, Violation, Unknown
    pub throughput_status: Option<String>,
    pub latency_status: Option<String>,
    pub availability_status: Option<String>,
    pub last_checked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub uid: Option<String>,
}
}
```

### 7. Configuration Management

#### Configuration Structure

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub crm: CrmManagerConfig, // Changed from CrmConfig to CrmManagerConfig
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrmManagerConfig {
    pub clusters: HashMap<String, CrmClientConfig>, // cluster_name -> config
    pub default_cluster: Option<String>, // Default cluster for deployments
    pub health_check_interval: Option<u64>, // seconds
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrmClientConfig {
    pub cluster_name: String,
    pub base_url: String,
    pub timeout: u64, // seconds
    pub retry_attempts: u32,
    pub authentication: Option<AuthConfig>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub auth_type: AuthType,
    pub token: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub certificate_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum AuthType {
    None,
    Bearer,
    Basic,
    Certificate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    pub insecure: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub timeout: u64, // seconds
    pub retry_timeout: u64, // seconds
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub storage_type: StorageType,
    pub etcd: Option<EtcdConfig>,
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum StorageType {
    Etcd,
    Memory,
}
```

## Implementation Plan

### Phase 1: Core Foundation (2-3 weeks)
1. **Project Setup**
   - Cargo workspace configuration
   - Basic dependency setup (tokio, serde, axum, sled)
   - Development environment setup

2. **Data Models**
   - Define core Rust structs with proper serialization
   - Implement validation logic
   - Create error types

3. **Storage Layer**
   - Implement etcd distributed storage
   - Create storage abstraction trait
   - Add basic CRUD operations with etcd integration

4. **CRD Generation Automation**
   - Create script to generate Kubernetes CRDs from Rust structs
   - Use `kube-derive` and `schemars` to auto-generate YAML manifests
   - Add `build.rs` script or `cargo xtask` command for CRD generation
   - Include validation schemas and OpenAPI specifications
   - Note: CRDs should be generated from shared `oprc-models` crate to ensure consistency between Package Manager and Class Runtime Manager

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
   - YAML configuration support
   - Environment-based overrides
   - Validation

### Phase 3: Deployment Orchestration (3-4 weeks)
1. **Deployment Service**
   - Deployment planning logic
   - Resource calculation
   - Environment management

2. **CRM Integration**
   - HTTP client implementation
   - Deployment unit creation
   - Status tracking

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
- `config` - Configuration management
- `tracing` - Structured logging
- `metrics` - Metrics collection

### Development Dependencies
- `tokio-test` - Testing utilities
- `wiremock` - HTTP mocking
- `testcontainers` - Integration testing
- `criterion` - Benchmarking

### CRD Generation Dependencies
- `kube-derive` - Kubernetes CRD generation from Rust structs
- `schemars` - JSON Schema generation for CRD validation
- `cargo-xtask` - Build automation tasks

**CRD Generation Automation Note**: 
Create automated scripts to generate Kubernetes Custom Resource Definitions (CRDs) from Rust data models. This ensures consistency between the Package Manager's data structures and the Kubernetes API objects that the Class Runtime Manager operates on. The generation process should:

1. **Extract CRD definitions** from `oprc-models` shared crate
2. **Generate YAML manifests** with proper validation schemas
3. **Include OpenAPI specifications** for kubectl integration
4. **Validate generated CRDs** against Kubernetes API server
5. **Update CRDs in deployment manifests** automatically

Example automation could use:
```bash
# cargo xtask generate-crds
# OR
# build.rs script that runs during compilation
# OR
# GitHub Actions workflow for CI/CD pipeline
```

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
- YAML files for complex configurations
- Kubernetes ConfigMaps for container deployments

#### Example etcd Configuration (YAML)

```yaml
# config/default.yaml
server:
  host: "0.0.0.0"
  port: 8080
  workers: 4

storage:
  storage_type: "Etcd"
  etcd:
    endpoints:
      - "etcd-1:2379"
      - "etcd-2:2379"
      - "etcd-3:2379"
    key_prefix: "/oaas/pm"
    username: "oaas-pm"
    password: "${ETCD_PASSWORD}"
    timeout: 30
    tls:
      ca_cert: "/etc/ssl/etcd/ca.pem"
      client_cert: "/etc/ssl/etcd/client.pem"
      client_key: "/etc/ssl/etcd/client-key.pem"
      insecure: false

crm:
  default_cluster: "production-east"
  health_check_interval: 30
  clusters:
    production-east:
      cluster_name: "production-east"
      base_url: "http://crm-prod-east:8081"
      timeout: 30
      retry_attempts: 3
      authentication:
        auth_type: "Bearer"
        token: "${CRM_PROD_EAST_TOKEN}"
      tls:
        enabled: true
        ca_cert_path: "/etc/ssl/crm/ca.pem"
        insecure: false
    production-west:
      cluster_name: "production-west"
      base_url: "http://crm-prod-west:8081"
      timeout: 30
      retry_attempts: 3
      authentication:
        auth_type: "Bearer"
        token: "${CRM_PROD_WEST_TOKEN}"
      tls:
        enabled: true
        ca_cert_path: "/etc/ssl/crm/ca.pem"
        insecure: false
    staging:
      cluster_name: "staging"
      base_url: "http://crm-staging:8081"
      timeout: 15
      retry_attempts: 2
      authentication:
        auth_type: "None"
      tls:
        enabled: false
        insecure: true
  circuit_breaker:
    failure_threshold: 5
    timeout: 60
    retry_timeout: 300

observability:
  log_level: "info"
  metrics_port: 9090
  jaeger_endpoint: "http://jaeger:14268/api/traces"
```

#### Environment Variables for Production

```bash
# etcd Configuration
ETCD_ENDPOINTS="etcd-1:2379,etcd-2:2379,etcd-3:2379"
ETCD_USERNAME="oaas-pm"
ETCD_PASSWORD="secure-password"
ETCD_KEY_PREFIX="/oaas/pm"

# TLS Configuration
ETCD_CA_CERT_PATH="/etc/ssl/etcd/ca.pem"
ETCD_CLIENT_CERT_PATH="/etc/ssl/etcd/client.pem"
ETCD_CLIENT_KEY_PATH="/etc/ssl/etcd/client-key.pem"

# CRM Multi-cluster Configuration
CRM_DEFAULT_CLUSTER="production-east"
CRM_PROD_EAST_TOKEN="prod-east-bearer-token"
CRM_PROD_WEST_TOKEN="prod-west-bearer-token"

# CRM Cluster URLs (can override YAML config)
CRM_PROD_EAST_URL="https://crm-prod-east.internal:8081"
CRM_PROD_WEST_URL="https://crm-prod-west.internal:8081" 
CRM_STAGING_URL="http://crm-staging.internal:8081"

# Server Configuration
SERVER_PORT=8080
LOG_LEVEL=info
ETCD_KEY_PREFIX="/oaas/pm"

# TLS Configuration
ETCD_CA_CERT_PATH="/etc/ssl/etcd/ca.pem"
ETCD_CLIENT_CERT_PATH="/etc/ssl/etcd/client.pem"
ETCD_CLIENT_KEY_PATH="/etc/ssl/etcd/client-key.pem"

# Server Configuration
SERVER_PORT=8080
LOG_LEVEL=info
```

## Multi-Cluster Deployment Strategies

### Cluster Selection Logic

The Package Manager supports several strategies for selecting target clusters for deployments:

1. **Explicit Cluster Targeting**: Classes can specify `target_clusters` in their deployment configuration
2. **Environment-based Mapping**: Clusters can be mapped to environments (e.g., `staging` → `staging-cluster`)
3. **Load Balancing**: Distribute deployments across available clusters based on current load
4. **Affinity Rules**: Deploy related classes to the same cluster for optimal performance
5. **Failover**: Automatically deploy to backup clusters if primary clusters are unavailable

### Deployment Unit Creation

When creating deployment units, the Package Manager:

1. **Evaluates Target Clusters**: Determines which clusters should receive the deployment
2. **Creates Per-Cluster Units**: Generates separate deployment units for each target cluster  
3. **Applies Cluster-Specific Configuration**: Adjusts resource requirements and networking for each cluster
4. **Maintains Cross-Cluster State**: Tracks deployment status across all clusters

### Health Monitoring

The CrmManager continuously monitors cluster health:

- **Periodic Health Checks**: Regular health pings to all CRM instances
- **Circuit Breaker Pattern**: Prevents cascading failures by isolating unhealthy clusters
- **Automatic Failover**: Redirects new deployments away from failed clusters
- **Recovery Detection**: Automatically re-enables clusters when they recover

### Benefits of Multi-Cluster Architecture

1. **High Availability**: Service continues even if one cluster fails
2. **Geographic Distribution**: Deploy closer to users for better performance
3. **Resource Isolation**: Separate workloads by environment or tenant
4. **Compliance**: Meet data residency requirements
5. **Scaling**: Distribute load across multiple Kubernetes clusters

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
