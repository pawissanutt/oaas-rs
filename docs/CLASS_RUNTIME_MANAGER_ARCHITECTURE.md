# Class Runtime Manager (CRM)

## Overview

The Class Runtime Manager (CRM) is a Kubernetes‑native controller that manages the deployment lifecycle of OaaS classes and functions. It receives deployment units from the Package Manager (PM) over gRPC, materializes Kubernetes resources using templates, tracks status via CRDs, and (optionally) enforces NFRs using metrics.

Key decision: maintain a single Kubernetes‑native code path for all environments (dev, edge, prod). Avoid alternate backends (e.g., Docker Compose) to prevent drift and maintenance overhead. Instead, CRM exposes runtime feature flags and “profiles” to run as a lightweight controller on single‑node Kubernetes (Docker Desktop, kind, k3d, k3s) without changing the architecture.

## Principles

- One binary, one reconciler path; behavior toggled by config.
- Single-cluster scope: one CRM instance manages exactly one Kubernetes cluster. For multi-cluster, deploy one CRM per cluster; CRM does not connect to or orchestrate multiple clusters.
- Kubernetes first: CRDs, server‑side apply, owner refs, conditions.
- Idempotent, level‑driven reconciliation; safe retries from PM.
- Observability‑first; enforcement can be passive or active.
- Minimal defaults for dev/edge, full capabilities for prod.

## Components

- CRDs: DeploymentRecord, DeploymentTemplate (Kubernetes API as source of truth)
- Controllers: deployment reconcile loop, template manager, resource manager
- gRPC server: accepts deploy/status/delete from PM (contract shared in `oprc-grpc`)
- NFR subsystem: optional analyzer + enforcement (HPA/Knative or direct spec patch)
- ODGM integration: optional sidecar or cluster integration per template

## Runtime profiles (configurable)

CRM supports three runtime profiles through environment variables. Profiles are presets over the same code path; they only change defaults.

- full (production)
  - NFR enforcement: active
  - HPA/KEDA: enabled (if available)
  - Prometheus metrics: required
  - Leader election: enabled
  - Knative templates: enabled (if present)
  - ODGM: enabled per template
- dev (single‑node developer experience)
  - NFR enforcement: off by default (observe‑only)
  - HPA/KEDA: disabled by default
  - Prometheus: optional; if absent, enforcement remains passive
  - Leader election: enabled by default (can be disabled)
  - Minimal resources and conservative requeues
  - ODGM: optional, can default to ephemeral storage
- edge (resource‑constrained runtime)
  - NFR enforcement: basic (CPU/throughput with hysteresis) or observe‑only
  - HPA/KEDA: optional; prefer HPA if available; otherwise static bounds
  - Prometheus: optional; reduced query frequency and cardinality
  - Leader election: enabled (single replica still safe)
  - ODGM: enabled with simplified settings (e.g., lower replication)

### Feature flags (runtime)

Use environment variables (via `envconfig`) to toggle behavior at runtime. Suggested keys (prefix `OPRC_CRM_` for consistency with the repo):

- OPRC_CRM_PROFILE = full | dev | edge
- OPRC_CRM_FEATURES_NFR_ENFORCEMENT = true|false
- OPRC_CRM_FEATURES_HPA = true|false
- OPRC_CRM_FEATURES_KNATIVE = true|false
- OPRC_CRM_FEATURES_PROMETHEUS = true|false
- OPRC_CRM_FEATURES_LEADER_ELECTION = true|false
- OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS = 60 (dev/edge higher)
- OPRC_CRM_ENFORCEMENT_MAX_REPLICA_DELTA = 30 (percent)
- OPRC_CRM_LIMITS_MAX_REPLICAS = 50
- OPRC_CRM_PROM_URL = http://prometheus.monitoring:9090 (optional in dev/edge)
- OPRC_CRM_K8S_NAMESPACE = default (auto from in‑cluster)
- OPRC_CRM_LOG = info|debug (default debug in dev)
- OPRC_CRM_SECURITY_MTLS = true|false (default false in dev, true in full)

These flags override profile defaults. Keep the implementation runtime‑driven (no compile‑time forks) so one image fits all. Optionally gate heavy dependencies (e.g., Prometheus client) behind Cargo features to reduce binary size, without changing behavior.

### Example config struct

```rust
#[derive(Envconfig, Clone, Debug)]
pub struct CrmConfig {
     #[envconfig(from = "OPRC_CRM_PROFILE", default = "dev")]
     pub profile: String,

     pub server: ServerConfig,
     pub k8s: K8sConfig,
     pub prometheus: PromConfig,
     pub security: SecurityConfig,
     pub features: FeaturesConfig,
     pub enforcement: EnforcementConfig,
}

#[derive(Envconfig, Clone, Debug)]
pub struct FeaturesConfig {
     #[envconfig(from = "OPRC_CRM_FEATURES_NFR_ENFORCEMENT", default = "false")]
     pub nfr_enforcement: bool,
     #[envconfig(from = "OPRC_CRM_FEATURES_HPA", default = "false")]
     pub hpa: bool,
     #[envconfig(from = "OPRC_CRM_FEATURES_KNATIVE", default = "false")]
     pub knative: bool,
     #[envconfig(from = "OPRC_CRM_FEATURES_PROMETHEUS", default = "false")]
     pub prometheus: bool,
     #[envconfig(from = "OPRC_CRM_FEATURES_LEADER_ELECTION", default = "true")]
     pub leader_election: bool,
}

#[derive(Envconfig, Clone, Debug)]
pub struct EnforcementConfig {
     #[envconfig(from = "OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS", default = "120")]
     pub cooldown_secs: u64,
     #[envconfig(from = "OPRC_CRM_ENFORCEMENT_MAX_REPLICA_DELTA", default = "30")]
     pub max_replica_delta_pct: u8,
     #[envconfig(from = "OPRC_CRM_LIMITS_MAX_REPLICAS", default = "20")]
     pub max_replicas: u32,
}
```

## Contracts and flows

### PM ↔ CRM gRPC contract

- Deploy(idempotent): create/update a DeploymentRecord; same deployment id must converge.
- Status: read from CRD status; CRM is the single source of truth; PM aggregates across clusters.
- Delete: mark for deletion; controller finalizer handles cleanup.
- Correlation IDs: propagate from PM to CRD annotations and logs.
- Retry‑safety: handlers must be idempotent.

Topology: PM talks to exactly one CRM per target cluster. CRM never manages or reaches into other clusters; it only uses in‑cluster configuration or a single kubeconfig context.

### CRDs

- DeploymentRecord (namespaced) with spec (deployment unit, selected template, NFR, resource needs, ODGM options) and status.
- DeploymentTemplate (namespaced or cluster‑scoped depending on tenancy) with template metadata, capabilities, manifests.
- Status subresource enabled; use Kubernetes Conditions (Available, Progressing, Degraded) with Reason/Message and `observedGeneration`.
- OwnerReferences on all child resources (Deployment/Service/ConfigMap/KnativeService/etc.). Finalizer only for external teardown (ODGM cluster, external handles).

Kubernetes configuration: `K8sConfig` uses in‑cluster config by default. For local development, a single kubeconfig context may be used. Multiple clusters or context lists are not supported by a single CRM instance.

## Reconciliation

- Level‑based and idempotent; prefer server‑side apply with field managers and conflict resolution.
- Conservative requeues in dev/edge; exponential backoff on API errors.
- Emit Kubernetes Events for major transitions (selected template, applied resources, enforcement action, failure).
- Concurrency limits and per‑namespace work‑queue bounds; avoid thundering herds.

## NFR analysis and enforcement (configurable)

Three modes controlled by feature flags:

- Off: skip enforcement; still compute and expose recommendations in status (observe‑only). Default in dev.
- Basic: adjust Deployment replicas or HPA targets with hysteresis and cooldown; small, bounded deltas; respect max replicas. Default in edge.
- Full: template‑aware actions (Knative min/max scale, HPA/KEDA, resource requests) driven by metrics. Default in full.

Safeguards:

- Stabilization windows, EMA smoothing, bounded deltas, max replicas, cool‑downs.
- Opt‑in per deployment via annotations (e.g., `oaas.io/enforcement=on|observe|off`).

Metrics:

- Prometheus integration optional (required in full). Reduce query frequency and label cardinality in dev/edge.

## ODGM integration (configurable)

- Template field controls ODGM enablement. For dev/edge, default to simplified settings: lower replication factor, ephemeral storage class, optional sidecar per pod.
- Readiness gates: function containers wait for ODGM ready when enabled.
- Anti‑affinity and PVCs applied in full profile; relaxed in dev/edge.
- Resharding: if unsupported dynamically, scale actions are gated or two‑phased.

## Security

- mTLS for PM↔CRM gRPC in full; optional in dev/edge.
- RBAC: least privilege; separate Role/ServiceAccount scoped to managed namespaces.
- Secrets via Kubernetes; no plaintext in env.

## Observability

- Tracing and metrics via `tracing` and Prometheus exporters.
- Controller metrics: reconcile durations, queue depth, requeues, failures.
- Log correlation with deployment id and PM request id.

## Single‑node Kubernetes (dev/edge)

- Supported runtimes: Docker Desktop Kubernetes, kind, k3d, k3s.
- Provide a dev overlay (Helm/Kustomize) with:
  - CRDs, CRM controller (single replica), leader election on, reduced resources
  - Prometheus optional; enforcement off by default
  - Knative optional; disabled by default
  - Namespace‑scoped RBAC and minimal requests/limits

## Testing strategy

- Unit: template selection, NFR math, config mapping.
- Integration: envtest/kind with CRDs; assert SSA patches, Conditions, owner refs.
- E2E (optional): with Prometheus stub in dev profile verifying observe‑only and basic enforcement.

## Migration and upgrades

- Start CRDs at v1alpha1; plan for conversion webhooks before GA.
- Rolling upgrades safe with leader election. Status and SSA field managers ensure consistency.

## Minimal implementation plan

1) Config + profiles
    - Add `CrmConfig` with profile + features + enforcement
    - Map profile → defaults; runtime flags override

2) CRDs + controller skeleton
    - DeploymentRecord/Template with status subresource + Conditions
    - Reconciler with SSA, owner refs, finalizers, Events

3) Resource templates
    - Basic Kubernetes Deployment/Service; optional Knative
    - ODGM sidecar wiring behind a flag

4) Optional enforcement
    - Observe‑only in dev; basic HPA/spec patch in edge; full in prod
    - Cooldown + hysteresis + bounds

5) Dev overlay
    - Single‑node manifests/Helm values for quick start
    - Docs for Docker Desktop/kind/k3d

This design keeps a single Kubernetes‑native path while offering a lightweight experience for developer and edge environments through configuration, not forks. It reduces operational risk and simplifies upgrades without limiting production capabilities.

# Class Runtime Manager (CRM) - Rust Architecture & Implementation Plan

## Overview

The Class Runtime Manager (CRM) is a Kubernetes-native component responsible for managing the deployment lifecycle of serverless functions and classes. Built in Rust for performance and reliability, it operates as a Kubernetes controller that receives deployment units from the Package Manager and manages them through Custom Resource Definitions (CRDs).

### 0. Shared Module Dependencies

The Class Runtime Manager leverages shared modules from the `common` crate collection:

- **oprc-grpc**: gRPC services and clients for inter-service communication (Package Manager, CRM, etc.)
- **oprc-models**: Common data models (OClassDeployment, NFR types, RuntimeState, DeploymentRecord)
- **oprc-cp-storage**: Control Plane storage abstraction layer with etcd implementation
- **oprc-observability**: Metrics collection and tracing setup
- **oprc-config**: Configuration loading and validation utilities

These shared modules ensure consistency across the OaaS platform and reduce code duplication. The gRPC clients for Package Manager communication are provided by `oprc-grpc` to maintain consistency across all services.

## Architectural Improvements & Design Decisions

### Key Design Principles

1. **Kubernetes-Native**: Leverage Kubernetes CRDs for state management and API-driven operations
2. **Controller Pattern**: Implement standard Kubernetes controller reconciliation loops
3. **Resource-Aware**: Intelligent resource allocation based on NFR requirements
4. **Multi-Environment**: Support for multiple deployment environments within the same cluster
5. **Template-Driven**: Pluggable deployment templates for different function types
6. **Observability-First**: Built-in metrics, logging, and health monitoring

### Architecture Benefits

- **State Consistency**: Kubernetes API server provides strong consistency guarantees
- **Native Integration**: Works seamlessly with existing Kubernetes tooling
- **Scalability**: Controller pattern naturally handles high-throughput scenarios
- **Fault Tolerance**: Kubernetes handles pod failures and rescheduling
- **Security**: Leverage Kubernetes RBAC and network policies

## Core Architecture

### 1. Application Structure

**Note**: The Class Runtime Manager should be organized within a `control-plane` directory alongside the Package Manager to clearly separate control plane components from data plane components (like ODGM). The recommended workspace structure is:

```
control-plane/
├── oprc-pm/                     // Package Manager
└── oprc-crm/                    // Class Runtime Manager (this component)
```

```
oprc-crm/
├── src/
│   ├── main.rs              // Application entry point
│   ├── lib.rs               // Library exports
│   ├── config/              // Configuration management
│   │   ├── mod.rs
│   │   ├── app.rs           // Application config
│   │   └── templates.rs     // Template configurations
│   ├── crd/                 // Custom Resource Definitions
│   │   ├── mod.rs
│   │   ├── deployment_record.rs  // DeploymentRecord CRD
│   │   ├── template.rs      // DeploymentTemplate CRD
│   │   └── environment.rs   // EnvironmentConfig CRD
│   ├── controller/          // Kubernetes controllers
│   │   ├── mod.rs
│   │   ├── deployment.rs    // Deployment controller
│   │   ├── template.rs      // Template controller
│   │   └── environment.rs   // Environment controller
│   ├── models/              // Data models
│   │   ├── mod.rs
│   │   ├── deployment.rs    // Deployment models
│   │   ├── nfr.rs          // NFR models
│   │   └── errors.rs        // Error types
│   ├── templates/           // Deployment templates
│   │   ├── mod.rs
│   │   ├── knative.rs       // Knative deployment template
│   │   ├── deployment.rs    // Standard Kubernetes deployment
│   │   └── job.rs           // Kubernetes Job template
│   ├── grpc/                // gRPC service implementations
│   │   ├── mod.rs
│   │   ├── deployment_service.rs  // Deployment gRPC service
│   │   ├── status_service.rs      // Status reporting service
│   │   └── health_service.rs      // Health check service
│   ├── reconciler/          // Reconciliation logic
│   │   ├── mod.rs
│   │   ├── deployment.rs    // Deployment reconciler
│   │   └── status.rs        // Status reporting
│   ├── nfr/                 // NFR analysis and enforcement
│   │   ├── mod.rs
│   │   ├── analyzer.rs      // NFR requirement analysis
│   │   ├── selector.rs      // Template selection logic
│   │   ├── validator.rs     // NFR validation
│   │   ├── enforcer.rs      // NFR enforcement engine
│   │   ├── monitor.rs       // Runtime metrics monitoring
│   │   └── actions.rs       // Enforcement actions (scaling, migration, etc.)
│   └── observability/       // Monitoring and logging
│       ├── mod.rs
│       ├── metrics.rs       // Prometheus metrics
│       └── events.rs        // Kubernetes events
├── tests/
│   ├── integration/
│   └── fixtures/
├── config/
│   ├── default.yaml
│   ├── development.yaml
│   └── production.yaml
├── k8s/                     // Kubernetes manifests
│   ├── crds/                // CRD definitions
│   ├── rbac/                // RBAC configurations
│   └── deployment/          // CRM deployment manifests
└── templates/               // Deployment template definitions
    ├── knative-template.yaml
    ├── deployment-template.yaml
    └── job-template.yaml
```

### 2. Custom Resource Definitions

#### DeploymentRecord CRD

```rust
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1",
    kind = "DeploymentRecord",
    plural = "deploymentrecords",
    status = "DeploymentRecordStatus",
    namespaced
)]
pub struct DeploymentRecordSpec {
    pub deployment_unit: DeploymentUnit,
    pub selected_template: String,
    pub target_environment: String,
    pub nfr_requirements: NfrRequirements,
    pub resource_allocation: ResourceAllocation,
    pub odgm_config: Option<OdgmDeploymentConfig>,
    pub function_containers: Vec<FunctionContainerSpec>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DeploymentRecordStatus {
    pub condition: DeploymentCondition,
    pub phase: DeploymentPhase,
    pub message: Option<String>,
    pub last_updated: String,
    pub resource_refs: Vec<ResourceReference>,
    pub nfr_compliance: NfrCompliance,
    pub odgm_cluster_status: Option<OdgmClusterStatus>,
    pub function_container_status: Vec<FunctionContainerStatus>,
}

// ODGM Integration Models
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct OdgmDeploymentConfig {
    pub enabled: bool,
    pub cluster_id: String,
    pub node_id_prefix: String,
    pub collections: Vec<OdgmCollectionConfig>,
    pub replication_factor: u8,
    pub shard_type: String, // ODGM shard type: "raft" (strong consistency), "basic" (eventual), "mst" (versioned)
    pub event_processing: EventProcessingConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct OdgmCollectionConfig {
    pub name: String,
    pub partition_count: u32,
    pub replica_count: u32,
    pub persistence_mode: PersistenceMode,
    pub access_patterns: Vec<AccessPattern>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct FunctionContainerSpec {
    pub function_key: String,
    pub container_image: String,
    pub container_type: ContainerType,
    pub resource_requirements: ResourceRequirements,
    pub environment_variables: std::collections::HashMap<String, String>,
    pub volume_mounts: Vec<VolumeMount>,
    pub health_check: HealthCheckConfig,
    pub odgm_integration: Option<OdgmIntegrationConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct OdgmIntegrationConfig {
    pub collections: Vec<String>,
    pub access_mode: AccessMode,
    pub caching_strategy: CachingStrategy,
    pub event_triggers: Vec<EventTriggerConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct OdgmClusterStatus {
    pub cluster_id: String,
    pub active_nodes: Vec<OdgmNodeStatus>,
    pub collections_status: Vec<OdgmCollectionStatus>,
    pub shard_distribution: ShardDistribution,
    pub consensus_status: ConsensusStatus,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct FunctionContainerStatus {
    pub function_key: String,
    pub container_status: ContainerStatus,
    pub resource_usage: ResourceUsage,
    pub health_status: HealthStatus,
    pub odgm_connection_status: Option<OdgmConnectionStatus>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub enum DeploymentCondition {
    Pending,
    Provisioning,
    Running,
    Scaling,
    Failed,
    Terminated,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub enum DeploymentPhase {
    TemplateSelection,
    ResourceProvisioning,
    PodScheduling,
    ServiceBinding,
    HealthChecking,
    Ready,
}
```

#### DeploymentTemplate CRD

```rust
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1",
    kind = "DeploymentTemplate",
    plural = "deploymenttemplates",
    namespaced
)]
pub struct DeploymentTemplateSpec {
    pub name: String,
    pub description: String,
    pub template_type: TemplateType,
    pub nfr_capabilities: NfrCapabilities,
    pub resource_requirements: ResourceRequirements,
    pub kubernetes_manifests: Vec<KubernetesManifest>,
    pub selection_criteria: SelectionCriteria,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub enum TemplateType {
    Knative,
    Deployment,
    StatefulSet,
    DaemonSet,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct NfrCapabilities {
    pub max_throughput: Option<u32>,
    pub min_latency: Option<u32>,
    pub max_latency: Option<u32>,
    pub auto_scaling: bool,
    pub persistent_storage: bool,
    pub network_policies: bool,
}
```

### 3. Deployment Controller

#### Controller Implementation

```rust
use kube::{
    api::{Api, ListParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType, Recorder, Reporter},
        finalizer::{finalizer, Event as Finalizer},
        watcher::Config,
    },
    CustomResource, Resource,
};
use std::sync::Arc;
use tokio::time::Duration;

pub struct DeploymentController {
    client: Client,
    templates: Arc<TemplateManager>,
    nfr_analyzer: Arc<NfrAnalyzer>,
    nfr_enforcer: Arc<NfrEnforcer>,
    resource_manager: Arc<ResourceManager>,
}

impl DeploymentController {
    pub fn new(
        client: Client,
        templates: Arc<TemplateManager>,
        nfr_analyzer: Arc<NfrAnalyzer>,
        nfr_enforcer: Arc<NfrEnforcer>,
        resource_manager: Arc<ResourceManager>,
    ) -> Self {
        Self {
            client,
            templates,
            nfr_analyzer,
            nfr_enforcer,
            resource_manager,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let api: Api<DeploymentRecord> = Api::all(self.client.clone());
        let context = Arc::new(ControllerContext {
            client: self.client.clone(),
            templates: self.templates.clone(),
            nfr_analyzer: self.nfr_analyzer.clone(),
            nfr_enforcer: self.nfr_enforcer.clone(),
            resource_manager: self.resource_manager.clone(),
        });

        // Start NFR enforcement loop in background
        let enforcer = self.nfr_enforcer.clone();
        tokio::spawn(async move {
            if let Err(e) = enforcer.start_enforcement_loop().await {
                tracing::error!("NFR enforcement loop failed: {:?}", e);
            }
        });

        Controller::new(api, Config::default())
            .shutdown_on_signal()
            .run(reconcile_deployment, error_policy, context)
            .for_each(|res| async move {
                match res {
                    Ok(o) => tracing::info!("Reconciled {:?}", o),
                    Err(e) => tracing::warn!("Reconcile failed: {:?}", e),
                }
            })
            .await;

        Ok(())
    }
}

async fn reconcile_deployment(
    deployment: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileError> {
    let namespace = deployment.namespace().unwrap_or_default();
    let name = deployment.name_any();
    
    tracing::info!("Reconciling DeploymentRecord {}/{}", namespace, name);

    // Apply finalizer for cleanup
    finalizer(&ctx.client, "oaas.io/deployment-finalizer", deployment, |event| async {
        match event {
            Finalizer::Apply(deployment) => reconcile_apply(deployment, ctx.clone()).await,
            Finalizer::Cleanup(deployment) => reconcile_cleanup(deployment, ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| ReconcileError::FinalizerError(Box::new(e)))
}

async fn reconcile_apply(
    deployment: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileError> {
    let spec = &deployment.spec;
    
    // 1. Analyze NFR requirements
    let nfr_analysis = ctx.nfr_analyzer.analyze(&spec.nfr_requirements).await?;
    
    // 2. Select appropriate template
    let template = ctx.templates.select_template(&nfr_analysis).await?;
    
    // 3. Generate Kubernetes resources with monitoring labels
    let mut resources = template.generate_resources(spec).await?;
    
    // 4. Add monitoring labels for Prometheus scraping
    for resource in &mut resources {
        resource.add_monitoring_labels(&spec.deployment_unit.id, &spec.nfr_requirements);
    }
    
    // 5. Apply resources to cluster
    ctx.resource_manager.apply_resources(&resources).await?;
    
    // 6. Set up NFR monitoring for this deployment
    ctx.nfr_enforcer.register_deployment_for_monitoring(&deployment).await?;
    
    // 7. Update status
    update_deployment_status(
        &ctx.client,
        &deployment,
        DeploymentCondition::Running,
        DeploymentPhase::Ready,
        "Deployment successful with NFR monitoring enabled",
    ).await?;
    
    Ok(Action::requeue(Duration::from_secs(300)))
}

async fn reconcile_cleanup(
    deployment: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileError> {
    tracing::info!("Cleaning up deployment: {}", deployment.name_any());
    
    // Delete all managed resources
    ctx.resource_manager.cleanup_resources(&deployment).await?;
    
    Ok(Action::await_change())
}

fn error_policy(
    _deployment: Arc<DeploymentRecord>,
    error: &ReconcileError,
    _ctx: Arc<ControllerContext>,
) -> Action {
    tracing::warn!("Reconcile error: {:?}", error);
    Action::requeue(Duration::from_secs(60))
}
```

### 4. NFR Analysis and Template Selection

#### NFR Analyzer

```rust
pub struct NfrAnalyzer {}

impl NfrAnalyzer {
    pub async fn analyze(&self, requirements: &NfrRequirements) -> Result<NfrAnalysis, NfrError> {
        let mut analysis = NfrAnalysis::default();
        
        // Analyze throughput requirements
        if let Some(throughput) = requirements.throughput {
            analysis.scaling_requirements = self.calculate_scaling_requirements(throughput)?;
        }
        
        // Analyze latency requirements
        if let Some(latency) = requirements.max_latency {
            analysis.performance_requirements = self.calculate_performance_requirements(latency)?;
        }
        
        // Analyze availability requirements
        if let Some(availability) = requirements.availability {
            analysis.reliability_requirements = self.calculate_reliability_requirements(availability)?;
        }
        
        Ok(analysis)
    }
    
    fn calculate_scaling_requirements(&self, throughput: u32) -> Result<ScalingRequirements, NfrError> {
        // Calculate required replicas based on throughput
        let replicas = (throughput / 1000).max(1); // Assume 1000 RPS per replica
        
        Ok(ScalingRequirements {
            min_replicas: 1,
            max_replicas: replicas * 2,
            target_cpu_utilization: 70,
            scale_up_behavior: ScaleBehavior::Fast,
            scale_down_behavior: ScaleBehavior::Gradual,
        })
    }
    
    fn calculate_performance_requirements(&self, max_latency: u32) -> Result<PerformanceRequirements, NfrError> {
        Ok(PerformanceRequirements {
            cpu_request: if max_latency < 100 { "500m" } else { "200m" }.to_string(),
            memory_request: if max_latency < 100 { "512Mi" } else { "256Mi" }.to_string(),
            startup_probe_config: StartupProbeConfig {
                initial_delay: 10,
                period: 5,
                timeout: 1,
                failure_threshold: 3,
            },
        })
    }
}
```

#### Template Manager

```rust
pub struct TemplateManager {
    templates: RwLock<Vec<DeploymentTemplate>>,
    client: Client,
}

impl TemplateManager {
    pub async fn select_template(&self, analysis: &NfrAnalysis) -> Result<Arc<DeploymentTemplate>, TemplateError> {
        let templates = self.templates.read().await;
        
        // Score templates based on NFR requirements
        let mut scored_templates: Vec<(f64, &DeploymentTemplate)> = templates
            .iter()
            .map(|template| {
                let score = self.calculate_template_score(template, analysis);
                (score, template)
            })
            .collect();
            
        // Sort by score (highest first)
        scored_templates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        
        // Select the best template
        scored_templates
            .first()
            .map(|(_, template)| Arc::new((*template).clone()))
            .ok_or(TemplateError::NoSuitableTemplate)
    }
    
    fn calculate_template_score(&self, template: &DeploymentTemplate, analysis: &NfrAnalysis) -> f64 {
        let mut score = 0.0;
        
        // Score based on scaling capabilities
        if template.spec.nfr_capabilities.auto_scaling && analysis.scaling_requirements.max_replicas > 1 {
            score += 30.0;
        }
        
        // Score based on performance characteristics
        if template.spec.template_type == TemplateType::Knative {
            score += 20.0; // Knative is good for serverless workloads
        }
        
        // Score based on resource efficiency
        score += self.calculate_resource_efficiency_score(template, analysis);
        
        score
    }
    
    fn calculate_resource_efficiency_score(&self, _template: &DeploymentTemplate, _analysis: &NfrAnalysis) -> f64 {
        // Calculate efficiency based on resource utilization
        25.0 // Placeholder
    }
}
```

### 4. NFR Enforcement System

The NFR enforcement system continuously monitors runtime metrics from Prometheus and automatically takes corrective actions when NFR violations are detected.

#### Core Components

1. **NFR Monitor**: Continuously queries Prometheus for deployment metrics
2. **Violation Detector**: Compares current metrics against NFR requirements
3. **Action Calculator**: Determines optimal scaling actions based on metrics
4. **Enforcement Engine**: Executes scaling and configuration changes

#### Throughput Enforcement Strategy

For throughput violations, the system uses a sophisticated calculation approach:

**Metrics Collection**:
- Current latency (P95) from HTTP request duration
- CPU utilization per pod
- Current throughput (requests/second)
- Memory utilization

**Optimal Replica Calculation**:
```
target_replicas = calculate_replicas_from_metrics(
    required_throughput,
    current_latency,
    cpu_utilization,
    target_latency_sla
)
```

**Algorithm**:
1. **Latency-based calculation**: If latency > SLA, increase replicas to distribute load
2. **CPU-based calculation**: If CPU > 70%, scale horizontally before vertical scaling
3. **Throughput gap analysis**: Calculate additional capacity needed
4. **Safety margins**: Add 20% buffer for traffic spikes

**Enforcement Actions**:
- **Knative Services**: Update `minScale` annotation to prevent scale-to-zero
- **Kubernetes Deployments**: Update replica count in deployment spec
- **HPA Configuration**: Adjust min/max replicas and target CPU utilization

#### Monitoring Queries

**Latency Monitoring**:
```
histogram_quantile(0.95, 
  rate(http_request_duration_seconds_bucket{deployment_id="..."}[5m])
)
```

**Throughput Monitoring**:
```
rate(http_requests_total{deployment_id="..."}[5m])
```

**CPU Utilization**:
```
rate(container_cpu_usage_seconds_total{deployment_id="..."}[5m]) * 100
```

#### Enforcement Loop

1. **Collection Phase** (every 30s): Query all metrics for active deployments
2. **Analysis Phase**: Compare against NFR requirements
3. **Decision Phase**: Calculate optimal configuration changes
4. **Execution Phase**: Apply changes to Kubernetes resources
5. **Verification Phase**: Monitor impact of changes

#### Configuration Example

```yaml
nfr_enforcement:
  check_interval: 30s
  prometheus_url: "http://prometheus:9090"
  enforcement_rules:
    throughput:
      violation_threshold: 0.8  # Trigger when below 80% of required
      scale_factor: 1.5         # Increase replicas by 50%
      max_replicas: 20
      cooldown_period: 300s
    latency:
      p95_threshold_ms: 500
      scale_on_violation: true
      cpu_scale_threshold: 0.7
```

### 5. gRPC Services for Package Manager Integration

The Class Runtime Manager exposes gRPC services for Package Manager communication and uses shared gRPC clients from `oprc-grpc` for outbound calls.

#### gRPC Service Implementation

```rust
// src/grpc/deployment_service.rs
use tonic::{Request, Response, Status};
use oprc_grpc::proto::deployment::*;
use oprc_grpc::server::deployment_server::DeploymentServiceHandler;
use async_trait::async_trait;

pub struct CrmDeploymentService {
    controller_context: Arc<ControllerContext>,
}

impl CrmDeploymentService {
    pub fn new(controller_context: Arc<ControllerContext>) -> Self {
        Self { controller_context }
    }
}

#[async_trait]
impl DeploymentServiceHandler for CrmDeploymentService {
    async fn deploy(
        &self,
        request: Request<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
        let req = request.into_inner();
        let deployment_unit = req.deployment_unit.ok_or(
            Status::invalid_argument("deployment_unit is required")
        )?;
        
        // Create DeploymentRecord CRD from deployment unit
        let deployment_record = self.create_deployment_record(deployment_unit).await
            .map_err(|e| Status::internal(format!("Failed to create deployment: {}", e)))?;
        
        // Apply to Kubernetes cluster
        let api: Api<DeploymentRecord> = Api::namespaced(
            self.controller_context.client.clone(),
            &deployment_record.metadata.namespace.as_ref().unwrap()
        );
        
        api.create(&PostParams::default(), &deployment_record).await
            .map_err(|e| Status::internal(format!("Failed to apply deployment: {}", e)))?;
        
        Ok(Response::new(DeployResponse {
            status: oaas::common::StatusCode::Ok as i32,
            deployment_id: deployment_record.metadata.name.unwrap(),
            message: Some("Deployment created successfully".to_string()),
        }))
    }
    
    async fn get_deployment_status(
        &self,
        request: Request<GetDeploymentStatusRequest>,
    ) -> Result<Response<GetDeploymentStatusResponse>, Status> {
        let req = request.into_inner();
        
        // Fetch deployment record from Kubernetes
        let api: Api<DeploymentRecord> = Api::all(self.controller_context.client.clone());
        let deployment = api.get(&req.deployment_id).await
            .map_err(|e| Status::not_found(format!("Deployment not found: {}", e)))?;
        
        let status = DeploymentStatusInfo {
            deployment_id: req.deployment_id,
            condition: deployment.status.as_ref()
                .map(|s| s.condition.clone().into())
                .unwrap_or(DeploymentCondition::Pending as i32),
            phase: deployment.status.as_ref()
                .map(|s| s.phase.clone().into())
                .unwrap_or(DeploymentPhase::TemplateSelection as i32),
            message: deployment.status.as_ref()
                .and_then(|s| s.message.clone()),
            nfr_compliance: deployment.status.as_ref()
                .map(|s| s.nfr_compliance.clone().into()),
        };
        
        Ok(Response::new(GetDeploymentStatusResponse {
            status: oaas::common::StatusCode::Ok as i32,
            deployment_status: Some(status),
        }))
    }
    
    async fn delete_deployment(
        &self,
        request: Request<DeleteDeploymentRequest>,
    ) -> Result<Response<DeleteDeploymentResponse>, Status> {
        let req = request.into_inner();
        
        let api: Api<DeploymentRecord> = Api::all(self.controller_context.client.clone());
        api.delete(&req.deployment_id, &DeleteParams::default()).await
            .map_err(|e| Status::internal(format!("Failed to delete deployment: {}", e)))?;
        
        Ok(Response::new(DeleteDeploymentResponse {
            status: oaas::common::StatusCode::Ok as i32,
            message: Some("Deployment deleted successfully".to_string()),
        }))
    }
}
```

#### Package Manager Communication

The CRM uses shared gRPC clients from `oprc-grpc` to communicate with the Package Manager:

```rust
// src/grpc/package_integration.rs
use oprc_grpc::client::package_client::PackageClient;
use oprc_models::{DeploymentStatus, RuntimeMetrics};

pub struct PackageManagerIntegration {
    client: PackageClient,
}

impl PackageManagerIntegration {
    pub async fn new(pm_endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = PackageClient::connect(pm_endpoint).await?;
        Ok(Self { client })
    }
    
    /// Report deployment status back to Package Manager
    pub async fn report_deployment_status(
        &mut self,
        deployment_id: String,
        status: DeploymentStatus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.client.report_deployment_status(deployment_id, status).await?;
        Ok(())
    }
    
    /// Report runtime metrics for NFR monitoring
    pub async fn report_runtime_metrics(
        &mut self,
        deployment_id: String,
        metrics: RuntimeMetrics,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert to protobuf and send
        let proto_metrics = metrics.into();
        self.client.report_runtime_metrics(deployment_id, proto_metrics).await?;
        Ok(())
    }
    
    /// Get package information for validation
    pub async fn get_package_info(
        &mut self,
        package_name: String,
    ) -> Result<oprc_grpc::proto::package::Package, Box<dyn std::error::Error>> {
        let response = self.client.get_package(package_name).await?;
        response.package.ok_or("Package not found".into())
    }
}
```

#### gRPC Server Setup

```rust
// src/grpc/server.rs
use tonic::transport::Server;
use oprc_grpc::server::deployment_server::DeploymentServiceServer;
use oprc_observability::middleware::grpc::MetricsInterceptor;

pub struct GrpcServer {
    deployment_service: CrmDeploymentService,
    address: std::net::SocketAddr,
}

impl GrpcServer {
    pub fn new(deployment_service: CrmDeploymentService, port: u16) -> Self {
        let address = ([0, 0, 0, 0], port).into();
        Self {
            deployment_service,
            address,
        }
    }
    
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let deployment_service = DeploymentServiceServer::new(self.deployment_service);
        
        tracing::info!("CRM gRPC server listening on {}", self.address);
        
        Server::builder()
            .add_service(tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(oprc_grpc::proto::FILE_DESCRIPTOR_SET)
                .build()?)
            .add_service(tonic_web::enable(deployment_service))
            .serve(self.address)
            .await?;
        
        Ok(())
    }
}
```

**Key Features**:
- **gRPC Service Endpoints**: Accepts deployment requests from Package Manager using standard protobuf definitions
- **Shared Client Usage**: Uses `oprc-grpc::client::PackageClient` for PM communication
- **Kubernetes Integration**: Creates DeploymentRecord CRDs from gRPC requests
- **Status Reporting**: Reports back deployment status and NFR compliance via gRPC
- **Metrics Integration**: Built-in observability through shared middleware

### 7. ODGM Integration and Configuration Generation

The Class Runtime Manager integrates deeply with the Object Data Grid Manager (ODGM) to provide stateful function execution with distributed data management.

#### 7.1 ODGM Deployment Architecture

**Multi-Container Deployment Pattern**:
```
OaaS Function Deployment (Pod)
├── Function Container (Business Logic)
│   ├── User function code
│   ├── OaaS runtime environment
│   ├── gRPC client to ODGM
│   └── Health check endpoints
├── ODGM Container (Data Layer)
│   ├── Object data grid manager
│   ├── Raft consensus engine
│   ├── Zenoh communication layer
│   ├── Event processing system
│   └── gRPC data service
└── Shared Volumes
    ├── /data/collections (Persistent data)
    └── /config (Configuration files)
```

#### 7.2 ODGM Configuration Generation

The CRM automatically generates ODGM configurations based on class specifications and deployment requirements:

**Key Insight**: ODGM uses partition IDs as part of object keys for data distribution. Objects are distributed across partitions based on a hash of their key, with the partition ID embedded in the key structure. This approach provides:
- **Automatic Load Distribution**: Objects are evenly distributed across partitions
- **Predictable Access Patterns**: Key-based routing to specific partitions
- **Scalable Partitioning**: More partitions allow better distribution for high-throughput scenarios

```rust
pub struct OdgmConfigGenerator {
    cluster_manager: Arc<ClusterManager>,
    node_id_allocator: Arc<NodeIdAllocator>,
}

impl OdgmConfigGenerator {
    pub async fn generate_config(
        &self,
        deployment: &DeploymentRecord,
        class_spec: &OClass,
        environment: &str,
    ) -> Result<OdgmDeploymentConfig, ConfigError> {
        let cluster_id = format!("{}-{}-{}", 
            deployment.spec.deployment_unit.id,
            environment,
            deployment.metadata.name.as_ref().unwrap()
        );
        
        let node_id_prefix = format!("crm-{}-{}", 
            environment,
            deployment.spec.deployment_unit.package_name
        );
        
        // Generate collections based on class state specifications
        let collections = self.generate_collections_config(class_spec, &deployment.spec.nfr_requirements)?;
        
        // Use replication factor from Package Manager (already calculated based on availability requirements)
        // The Package Manager calculates replication_factor in ResourceAllocation based on:
        // - availability >= 0.999 => replication_factor = 3 (99.9% availability)
        // - availability >= 0.99 => replication_factor = 2 (99% availability)  
        // - default => replication_factor = 1 (basic availability)
        let replication_factor = deployment.spec.resource_allocation.replication_factor;
        
        // Configure event processing based on function bindings
        let event_processing = self.configure_event_processing(class_spec, &deployment.spec.nfr_requirements)?;
        
        Ok(OdgmDeploymentConfig {
            enabled: true,
            cluster_id,
            node_id_prefix,
            collections,
            replication_factor,
            shard_type: self.determine_shard_type(&deployment.spec.nfr_requirements, replication_factor)?,
            event_processing,
        })
    }
    
    fn generate_collections_config(&self, class_spec: &OClass, nfr: &NfrRequirements) -> Result<Vec<OdgmCollectionConfig>, ConfigError> {
        let mut collections = Vec::new();
        
        // Determine partition count based on throughput requirements
        let partition_count = self.determine_partition_count(nfr)?;
        
        // Generate collections based on class requirements
        if let Some(state_spec) = &class_spec.state_spec {
            // Determine collection configuration based on actual requirements
            let (collection_name, effective_partition_count, replica_count, access_patterns) = 
                if state_spec.requires_singleton {
                    // Singleton state: single partition for consistency
                    (format!("{}_singleton", class_spec.key), 1, 3, vec![AccessPattern::ReadWrite])
                } else if state_spec.supports_batch_operations {
                    // Collection-like state: more partitions and batch read support
                    (format!("{}_collection", class_spec.key), 
                     partition_count.max(4), 3, 
                     vec![AccessPattern::ReadWrite, AccessPattern::BatchRead])
                } else {
                    // Normal state: standard configuration
                    (format!("{}_data", class_spec.key), 
                     partition_count, 2, 
                     vec![AccessPattern::ReadWrite])
                };

            collections.push(OdgmCollectionConfig {
                name: collection_name,
                partition_count: effective_partition_count,
                replica_count,
                persistence_mode: PersistenceMode::Persistent,
                access_patterns,
            });
        }
        
        Ok(collections)
    }
    
    fn determine_partition_count(&self, nfr: &NfrRequirements) -> Result<u32, ConfigError> {
        // ODGM uses partition IDs as part of object keys for distribution
        // More partitions = better distribution but more overhead
        if let Some(throughput) = nfr.throughput {
            match throughput {
                0..=1000 => Ok(1),    // Single partition for low throughput
                1001..=5000 => Ok(4), // Standard partitioning
                5001..=20000 => Ok(8), // Higher partitioning for medium-high throughput
                _ => Ok(16),          // Maximum partitioning for very high throughput
            }
        } else {
            Ok(4) // Default partition count
        }
    }
    
    fn determine_shard_type(&self, nfr: &NfrRequirements, replication_factor: u8) -> Result<String, ConfigError> {
        // ODGM uses shard_type to determine consistency level:
        // - "raft": Strong consistency with Raft consensus (for replicated data)
        // - "basic": Eventually consistent (for single replica, high-throughput scenarios)
        // - "mst": Merkle Search Tree for version-controlled data
        
        if replication_factor > 1 {
            // Multiple replicas require consensus for consistency
            if let Some(latency) = nfr.max_latency {
                if latency < 50 {
                    // Very low latency with replication - use basic and accept eventual consistency
                    Ok("basic".to_string())
                } else {
                    // Standard latency with replication - use Raft for strong consistency
                    Ok("raft".to_string())
                }
            } else {
                // Default to Raft for replicated data
                Ok("raft".to_string())
            }
        } else {
            // Single replica - can use basic consistency for better performance
            Ok("basic".to_string())
        }
    }
    
    fn configure_event_processing(
        &self, 
        class_spec: &OClass, 
        nfr: &NfrRequirements
    ) -> Result<EventProcessingConfig, ConfigError> {
        let max_trigger_depth = if nfr.max_latency.unwrap_or(1000) < 100 {
            5  // Lower depth for low-latency requirements
        } else {
            10 // Standard depth
        };
        
        let trigger_timeout = if let Some(latency) = nfr.max_latency {
            (latency as u64 / 2).max(1000) // Half of max latency, minimum 1s
        } else {
            5000 // Default 5s timeout
        };
        
        // Generate event triggers based on function bindings
        let mut event_triggers = Vec::new();
        for function_binding in &class_spec.functions {
            if function_binding.trigger_on_state_change {
                event_triggers.push(EventTriggerConfig {
                    function_key: function_binding.function_key.clone(),
                    trigger_type: TriggerType::StateChange,
                    collection_filters: vec![format!("{}_*", class_spec.key)],
                    async_execution: function_binding.async_execution.unwrap_or(false),
                });
            }
        }
        
        Ok(EventProcessingConfig {
            enabled: !event_triggers.is_empty(),
            max_trigger_depth,
            trigger_timeout_ms: trigger_timeout,
            event_triggers,
            batch_processing: BatchProcessingConfig {
                enabled: true,
                batch_size: 100,
                batch_timeout_ms: 1000,
            },
        })
    }
}
```

#### 7.3 Container Configuration Generation

**Function Container Environment Variables**:
```rust
pub fn generate_function_container_env(
    &self,
    deployment: &DeploymentRecord,
    function_spec: &FunctionContainerSpec,
    odgm_config: &OdgmDeploymentConfig,
) -> Result<std::collections::HashMap<String, String>, ConfigError> {
    let mut env_vars = std::collections::HashMap::new();
    
    // ODGM Connection Configuration
    env_vars.insert("ODGM_ENDPOINT".to_string(), "http://localhost:8080".to_string());
    env_vars.insert("ODGM_CLUSTER_ID".to_string(), odgm_config.cluster_id.clone());
    env_vars.insert("ODGM_NODE_ROLE".to_string(), "client".to_string());
    
    // Function-specific Configuration
    env_vars.insert("FUNCTION_KEY".to_string(), function_spec.function_key.clone());
    env_vars.insert("DEPLOYMENT_ID".to_string(), deployment.spec.deployment_unit.id.clone());
    env_vars.insert("ENVIRONMENT".to_string(), deployment.spec.target_environment.clone());
    
    // Collection Access Configuration
    if let Some(odgm_integration) = &function_spec.odgm_integration {
        let collections_json = serde_json::to_string(&odgm_integration.collections)?;
        env_vars.insert("ODGM_COLLECTIONS".to_string(), collections_json);
        env_vars.insert("ODGM_ACCESS_MODE".to_string(), odgm_integration.access_mode.to_string());
        env_vars.insert("ODGM_CACHING_STRATEGY".to_string(), odgm_integration.caching_strategy.to_string());
    }
    
    // Performance Configuration
    if let Some(throughput) = deployment.spec.nfr_requirements.throughput {
        env_vars.insert("EXPECTED_THROUGHPUT".to_string(), throughput.to_string());
    }
    if let Some(latency) = deployment.spec.nfr_requirements.max_latency {
        env_vars.insert("MAX_LATENCY_MS".to_string(), latency.to_string());
    }
    
    Ok(env_vars)
}
```

**ODGM Container Environment Variables**:
```rust
pub fn generate_odgm_container_env(
    &self,
    deployment: &DeploymentRecord,
    odgm_config: &OdgmDeploymentConfig,
    replica_index: u32,
) -> Result<std::collections::HashMap<String, String>, ConfigError> {
    let mut env_vars = std::collections::HashMap::new();
    
    // Core ODGM Configuration
    env_vars.insert("ODGM_HTTP_PORT".to_string(), "8080".to_string());
    env_vars.insert("ODGM_NODE_ID".to_string(), 
        format!("{}{:03}", odgm_config.node_id_prefix, replica_index));
    env_vars.insert("ODGM_NODE_ADDR".to_string(), "http://0.0.0.0:8080".to_string());
    
    // Cluster Configuration
    let member_nodes = self.generate_cluster_members(&odgm_config, replica_index)?;
    env_vars.insert("ODGM_MEMBERS".to_string(), member_nodes.join(","));
    
    // Collection Configuration
    let collections_config = serde_json::to_string(&odgm_config.collections)?;
    env_vars.insert("ODGM_COLLECTION".to_string(), collections_config);
    
    // Event Processing Configuration
    env_vars.insert("ODGM_EVENTS_ENABLED".to_string(), 
        odgm_config.event_processing.enabled.to_string());
    env_vars.insert("ODGM_MAX_TRIGGER_DEPTH".to_string(), 
        odgm_config.event_processing.max_trigger_depth.to_string());
    env_vars.insert("ODGM_TRIGGER_TIMEOUT_MS".to_string(), 
        odgm_config.event_processing.trigger_timeout_ms.to_string());
    
    // Performance Configuration
    env_vars.insert("ODGM_MAX_SESSIONS".to_string(), "4".to_string());
    env_vars.insert("ODGM_REFLECTION_ENABLED".to_string(), "false".to_string());
    
    // Logging Configuration
    let log_level = if deployment.spec.target_environment == "dev" { "DEBUG" } else { "INFO" };
    env_vars.insert("ODGM_LOG".to_string(), 
        format!("{},openraft=info,zenoh=info,h2=warn", log_level));
    
    // Zenoh Configuration
    env_vars.insert("OPRC_ZENOH_PORT".to_string(), "0".to_string()); // Auto-assign
    
    Ok(env_vars)
}
```

#### 7.4 Kubernetes Manifest Generation

**Pod Specification with ODGM Integration**:
```rust
pub fn generate_pod_spec(
    &self,
    deployment: &DeploymentRecord,
    odgm_config: &OdgmDeploymentConfig,
) -> Result<k8s_openapi::api::core::v1::PodSpec, ManifestError> {
    let mut containers = Vec::new();
    
    // Generate ODGM container
    let odgm_container = Container {
        name: "odgm".to_string(),
        image: Some("oprc/odgm:latest".to_string()),
        ports: Some(vec![
            ContainerPort {
                name: Some("grpc".to_string()),
                container_port: 8080,
                protocol: Some("TCP".to_string()),
                ..Default::default()
            },
            ContainerPort {
                name: Some("zenoh".to_string()),
                container_port: 7447,
                protocol: Some("TCP".to_string()),
                ..Default::default()
            },
        ]),
        env: Some(self.generate_odgm_container_env(deployment, odgm_config, 0)?
            .into_iter()
            .map(|(k, v)| EnvVar { name: k, value: Some(v), ..Default::default() })
            .collect()),
        volume_mounts: Some(vec![
            VolumeMount {
                name: "odgm-data".to_string(),
                mount_path: "/data".to_string(),
                ..Default::default()
            },
            VolumeMount {
                name: "odgm-config".to_string(),
                mount_path: "/config".to_string(),
                ..Default::default()
            },
        ]),
        resources: Some(ResourceRequirements {
            requests: Some([
                ("cpu".to_string(), Quantity("200m".to_string())),
                ("memory".to_string(), Quantity("512Mi".to_string())),
            ].into()),
            limits: Some([
                ("cpu".to_string(), Quantity("1000m".to_string())),
                ("memory".to_string(), Quantity("1Gi".to_string())),
            ].into()),
        }),
        ..Default::default()
    };
    containers.push(odgm_container);
    
    // Generate function containers
    for function_spec in &deployment.spec.function_containers {
        let function_container = Container {
            name: format!("function-{}", function_spec.function_key),
            image: Some(function_spec.container_image.clone()),
            env: Some(self.generate_function_container_env(deployment, function_spec, odgm_config)?
                .into_iter()
                .map(|(k, v)| EnvVar { name: k, value: Some(v), ..Default::default() })
                .collect()),
            volume_mounts: Some(vec![
                VolumeMount {
                    name: "odgm-config".to_string(),
                    mount_path: "/config".to_string(),
                    read_only: Some(true),
                    ..Default::default()
                },
            ]),
            resources: Some(function_spec.resource_requirements.clone().into()),
            ..Default::default()
        };
        containers.push(function_container);
    }
    
    Ok(PodSpec {
        containers,
        volumes: Some(vec![
            Volume {
                name: "odgm-data".to_string(),
                persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                    claim_name: format!("{}-odgm-data", deployment.metadata.name.as_ref().unwrap()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Volume {
                name: "odgm-config".to_string(),
                config_map: Some(ConfigMapVolumeSource {
                    name: Some(format!("{}-odgm-config", deployment.metadata.name.as_ref().unwrap())),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ]),
        restart_policy: Some("Always".to_string()),
        ..Default::default()
    })
}
```

#### 7.5 ODGM Health Monitoring

**Health Check Implementation**:
```rust
pub async fn check_odgm_health(
    &self,
    deployment: &DeploymentRecord,
) -> Result<OdgmClusterStatus, HealthCheckError> {
    let odgm_endpoint = format!("http://{}-odgm:8080", 
        deployment.metadata.name.as_ref().unwrap());
    
    // Check ODGM gRPC health
    let health_client = HealthClient::connect(odgm_endpoint.clone()).await?;
    let health_response = health_client.check(HealthCheckRequest {
        service: "odgm".to_string(),
    }).await?;
    
    // Check cluster consensus status
    let data_client = DataServiceClient::connect(odgm_endpoint).await?;
    let cluster_info = data_client.get_cluster_info(GetClusterInfoRequest {}).await?;
    
    // Check collection status
    let mut collections_status = Vec::new();
    for collection_config in &deployment.spec.odgm_config.as_ref().unwrap().collections {
        let collection_status = data_client.get_collection_status(GetCollectionStatusRequest {
            collection_name: collection_config.name.clone(),
        }).await?;
        
        collections_status.push(OdgmCollectionStatus {
            name: collection_config.name.clone(),
            partition_count: collection_status.partition_count,
            active_partitions: collection_status.active_partitions,
            total_objects: collection_status.total_objects,
            health_status: if collection_status.active_partitions == collection_status.partition_count {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded
            },
        });
    }
    
    Ok(OdgmClusterStatus {
        cluster_id: cluster_info.cluster_id,
        active_nodes: cluster_info.nodes.into_iter().map(|n| OdgmNodeStatus {
            node_id: n.node_id,
            address: n.address,
            status: n.status,
            last_heartbeat: n.last_heartbeat,
        }).collect(),
        collections_status,
        shard_distribution: ShardDistribution {
            total_shards: cluster_info.total_shards,
            active_shards: cluster_info.active_shards,
            distribution_balance: cluster_info.balance_score,
        },
        consensus_status: ConsensusStatus {
            leader_node: cluster_info.leader_node,
            term: cluster_info.current_term,
            committed_index: cluster_info.committed_index,
        },
    })
}
```

This ODGM integration provides:

**Key Benefits**:
1. **Stateful Function Execution**: Functions can maintain state across invocations through ODGM collections
2. **Distributed Consistency**: Raft consensus ensures data consistency across replicas
3. **Event-Driven Processing**: Functions can trigger on data changes through ODGM events
4. **High Availability**: Automatic failover and data replication
5. **Performance Optimization**: Local ODGM instance per pod reduces network latency

**Core Configuration Areas**:
- **Server Configuration**: HTTP server settings, ports, workers
- **Kubernetes Configuration**: Namespace, service account, RBAC settings
- **Template Configuration**: Template directory, validation rules
- **NFR Configuration**: Default resource limits, scaling thresholds
- **Prometheus Configuration**: Metrics endpoint, query intervals

## Implementation Plan

### Phase 1: Foundation (3-4 weeks)
1. **Project Setup**
   - Cargo workspace with kube-rs dependencies
   - CRD definitions and code generation
   - Basic Kubernetes client setup

2. **Core Models**
   - Rust structs for all CRDs
   - Serialization/deserialization
   - Validation logic

3. **Basic Controller**
   - Simple reconciliation loop
   - Status updates
   - Error handling

### Phase 2: Template System (2-3 weeks)
1. **Template Engine**
   - Template loading and validation
   - Kubernetes manifest generation
   - Template selection logic

2. **NFR Analysis**
   - Requirement analysis
   - Resource calculation
   - Template scoring

3. **Resource Management**
   - Kubernetes resource creation
   - Resource lifecycle management
   - Cleanup procedures

### Phase 3: Advanced Features (3-4 weeks)
1. **Multi-Environment Support**
   - Environment configuration
   - Resource isolation
   - Environment-specific templates

2. **Scaling and Performance**
   - HPA integration
   - Resource optimization
   - Performance monitoring

3. **API Integration**
   - HTTP API for Package Manager
   - Event publishing
   - Status reporting

### Phase 4: Production Readiness (2-3 weeks)
1. **Observability**
   - Prometheus metrics
   - Structured logging
   - Health checks

2. **Security**
   - RBAC configuration
   - Network policies
   - Secret management

3. **Testing**
   - Unit tests
   - Integration tests
   - E2E tests

## Key Dependencies

- **kube-rs**: Kubernetes client and controller framework
- **tokio**: Async runtime for Rust
- **axum**: HTTP server framework
- **prometheus**: Metrics collection and querying
- **serde**: Serialization for CRDs and API
- **tracing**: Structured logging and observability

## Deployment Architecture

### Kubernetes Resources Required

**Custom Resource Definitions**:
- DeploymentRecord: Tracks deployment state and NFR requirements
- DeploymentTemplate: Defines reusable deployment patterns
- EnvironmentConfig: Environment-specific configurations

**RBAC Permissions**:
- Read/Write access to OaaS CRDs
- Manage Deployments, Services, ConfigMaps
- Query Prometheus metrics
- Create/update Knative services

**Controller Deployment**:
- Single replica with leader election capability
- Service account with appropriate RBAC
- Prometheus metrics endpoint on port 9090
- Health check endpoints for Kubernetes probes

## Configuration Management

**Core Configuration Areas**:
- **Server Configuration**: HTTP server settings, ports, workers
- **Kubernetes Configuration**: Namespace, service account, RBAC settings
- **Template Configuration**: Template directory, validation rules
- **NFR Configuration**: Default resource limits, scaling thresholds
- **Prometheus Configuration**: Metrics endpoint, query intervals
- **ODGM Configuration**: Cluster settings, collection defaults, event processing

**ODGM-Specific Configuration**:
```yaml
# config/production.yaml
odgm:
  # Default ODGM settings for all deployments
  defaults:
    cluster_id_prefix: "oaas-prod"
    node_id_prefix: "crm-node"
    replication_factor: 3
    consistency_level: "strong"
    event_processing:
      enabled: true
      max_trigger_depth: 10
      trigger_timeout_ms: 5000
      batch_processing:
        enabled: true
        batch_size: 100
        batch_timeout_ms: 1000
  
  # Collection configuration templates
  collection_templates:
    normal_state:
      partition_count: 4
      replica_count: 2
      persistence_mode: "persistent"
      access_patterns: ["read_write"]
    
    collection_state:
      partition_count: 8
      replica_count: 3
      persistence_mode: "persistent" 
      access_patterns: ["read_write", "batch_read"]
    
    singleton_state:
      partition_count: 1
      replica_count: 3
      persistence_mode: "persistent"
      access_patterns: ["read_write"]
  
  # Partition count calculation based on throughput
  partition_strategies:
    low_throughput: 1      # 0-1000 RPS
    standard: 4            # 1001-5000 RPS  
    high_throughput: 8     # 5001-20000 RPS
    very_high: 16          # 20000+ RPS
  
  # Container resource allocation
  container_resources:
    odgm:
      requests:
        cpu: "200m"
        memory: "512Mi"
      limits:
        cpu: "1000m"
        memory: "1Gi"
    
    function:
      requests:
        cpu: "100m"
        memory: "256Mi"
      limits:
        cpu: "500m"
        memory: "512Mi"
  
  # Health check configuration
  health_checks:
    odgm:
      initial_delay_seconds: 30
      period_seconds: 10
      timeout_seconds: 5
      failure_threshold: 3
    
    function:
      initial_delay_seconds: 10
      period_seconds: 5
      timeout_seconds: 3
      failure_threshold: 3
```

## Monitoring and Observability

**Key Metrics**:
- `crm_deployments_total`: Total deployments processed
- `crm_active_deployments`: Currently active deployments
- `crm_nfr_violations_total`: NFR violations detected
- `crm_enforcement_actions_total`: Enforcement actions taken
- `crm_reconciliation_duration_seconds`: Controller performance

**Logging**:
- Structured JSON logs with deployment correlation IDs
- NFR violation events with metrics context
- Enforcement action results and impacts

This streamlined architecture focuses on the essential components while maintaining the sophisticated NFR enforcement approach using Prometheus metrics to calculate optimal scaling decisions.
