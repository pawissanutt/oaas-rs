# Class Runtime Manager (CRM) - Self-Contained Implementation Specification

## Overview

The Class Runtime Manager (CRM) is a critical component of the Object-as-a-Service (OaaS) serverless platform responsible for managing the runtime deployment and lifecycle of serverless functions and classes. It receives `DeploymentUnit` objects from the Package Manager and executes deployments while enforcing Non-Functional Requirements (NFRs) through template selection and configuration.

## Architecture Changes

### Current Architecture (PM-Dependent)
- CRM depends on Package Manager's gRPC data services (`CrStateService`, `InternalCrStateService`)
- Deployment state stored centrally in Package Manager
- CRM acts as a stateless executor receiving deployment commands

### New Architecture (Self-Contained)
- CRM manages local deployment data independently using Kubernetes CRDs
- Autonomous deployment lifecycle management with local state persistence
- Direct API communication with Package Manager for deployment units
- NFR-driven template selection and enforcement
- Local monitoring and health tracking

## Core Functionality

The CRM provides the following key capabilities:

### 1. Deployment Management
- **Receive DeploymentUnits**: Accept deployment requests from Package Manager
- **Template Selection**: Choose optimal deployment template based on NFR requirements
- **Resource Provisioning**: Deploy functions with appropriate resource allocations
- **Status Tracking**: Monitor deployment status and report back to Package Manager
- **Lifecycle Management**: Handle deployment updates, scaling, and deletion

### 2. NFR Enforcement
- **Performance Guarantees**: Enforce throughput and latency requirements
- **Resource Constraints**: Apply CPU, memory, and scaling limits
- **Template Matching**: Select templates that can meet specified NFRs
- **Constraint Validation**: Reject deployments that cannot meet hard constraints
- **Runtime Monitoring**: Track NFR compliance during execution

### 3. Multi-Environment Support
- **Environment Isolation**: Deploy to multiple environments (dev, staging, prod)
- **Environment-Specific Configuration**: Apply environment-specific settings
- **Resource Quotas**: Enforce per-environment resource limits
- **Health Monitoring**: Track environment-specific deployment health

### 4. State Management
- **Local Persistence**: Store deployment state in Kubernetes CRDs
- **Recovery**: Restore state after CRM restarts or failures
- **Audit Trail**: Maintain deployment history and changes

## Core Components

### 1. Data Management Layer

#### 1.1 Data Models

**DeploymentRecord** (Primary CRD for local storage)
```json
{
  "id": "string",                  // TSID-based unique identifier
  "packageName": "string",         // Source package identifier (from OPackage.name)
  "environment": "string",         // Target environment name
  "status": "DeploymentStatus",    // PENDING, ACTIVE, FAILED, DELETED
  "attachedClasses": {             // Map of deployed classes from OClass
    "classKey": {
      "_key": "string",            // Unique class identifier
      "_rev": "string",            // Package Manager revision
      "name": "string",            // Human-readable class name
      "pkg": "string",             // Parent package name
      "stateType": "enum",         // NORMAL, COLLECTION, SINGLETON
      "stateSpec": {               // State structure definition
        "keySpecs": [
          {
            "name": "string",
            "type": "enum",        // STRING, INTEGER, DOUBLE, BOOLEAN
            "optional": "boolean"
          }
        ],
        "defaultProvider": "string"
      },
      "functions": [               // Function bindings for this class
        {
          "name": "string",        // Binding name
          "function": "string",    // Function key reference
          "access": "enum",        // PUBLIC, INTERNAL, PRIVATE
          "defaultRoute": "boolean",
          "condition": "string",   // Conditional binding expression
          "immutable": "boolean"   // Immutability flag
        }
      ],
      "refSpec": [],               // References to other classes
      "parents": ["string"],       // Inheritance hierarchy
      "description": "string",     // Documentation
      "disabled": "boolean",       // Availability flag
      "markForRemoval": "boolean"  // Deletion flag
    }
  },
  "attachedFunctions": {           // Map of deployed functions from OFunction
    "functionKey": {
      "_key": "string",            // Unique function identifier
      "name": "string",            // Function name
      "pkg": "string",             // Parent package
      "description": "string",     // Function documentation
      "type": "enum",              // BUILTIN, TASK, MACRO, LOGICAL
      "outputCls": "string",       // Output class type
      "macro": {                   // Macro function specification
        "steps": []                // Dataflow steps for MACRO functions
      },
      "provision": {               // Runtime provisioning config
        "knative": {
          "minScale": "int",       // Minimum replicas
          "maxScale": "int",       // Maximum replicas
          "cpu": "string",         // CPU allocation
          "memory": "string",      // Memory allocation
          "env": {"key": "value"}, // Environment variables
          "requestsCpu": "string", // CPU requests
          "requestsMemory": "string", // Memory requests
          "image": "string",       // Container image
          "pullPolicy": "string"   // Image pull policy
        },
        "docker": {                // Docker-specific config
          "image": "string",
          "port": "int",
          "env": {"key": "value"}
        }
      },
      "variableDescriptions": [],  // Variable descriptions
      "status": {                  // Deployment status from PM
        "condition": "enum",       // PENDING, DEPLOYING, RUNNING, DOWN, DELETED
        "invocationUrl": "string", // Function endpoint URL
        "errorMsg": "string"       // Error message if failed
      },
      "state": "enum",             // ENABLED, DISABLED, FROZEN
      "requirements": {            // QoS requirements for NFR enforcement
        "throughput": "int",       // Requests per second
        "latency": "int",          // Maximum latency (ms)
        "availability": "float"    // Availability percentage (handled by PM)
      },
      "constraints": {             // QoS constraints for NFR enforcement
        "hardLatency": "int",      // Hard latency limit (ms)
        "maxConcurrency": "int"    // Maximum concurrent executions
      },
      "config": {                  // Function-specific configuration
        "timeout": "int",          // Execution timeout (ms)
        "env": {"key": "value"}    // Environment variables
      },
      "immutable": "boolean"       // Immutability flag
    }
  },
  "deploymentPlan": {              // CRM-generated deployment plan
    "template": "string",          // Selected template name
    "components": {
      "LOADBALANCER": "ComponentPlan",
      "INVOKER": "ComponentPlan",
      "CONFIG": "ComponentPlan"
    },
    "functionPlans": {
      "functionKey": "FunctionResourcePlan"
    }
  },
  "oClassDeployment": {            // Deployment configuration from PM
    "_key": "string",              // Deployment identifier
    "partitionCount": "int",       // Number of data partitions
    "replicaCount": "int",         // Number of replicas per partition (availability)
    "targetEnvs": ["string"],      // Target environment names
    "members": [                   // Member group configurations
      {
        "id": "int",
        "env": "string",           // Environment name
        "maxShards": "int",        // Maximum shards (-1 = unlimited)
        "allPartitions": "boolean", // Handle all partitions flag
        "disabledFn": ["string"],  // Disabled functions
        "standbyFn": ["string"],   // Standby functions
        "forceTemplate": "string"  // Forced template name for NFR
      }
    ],
    "shardType": "string",         // Sharding strategy (basic, etc.)
    "assignments": [               // Shard assignments
      {
        "primary": "int",          // Primary member ID
        "owners": ["int"],         // Owner member IDs
        "shardIds": ["int"]        // Assigned shard IDs
      }
    ],
    "options": {"key": "value"}    // Additional deployment options
  },
  "runtimeState": {                // Runtime execution state
    "clsKey": "string",            // Associated class key
    "env": "string",               // Environment name
    "partitions": [                // Partition information
      {
        "id": "int",
        "replicas": ["string"]     // Node identifiers
      }
    ],
    "status": "enum",              // RUNNING, DOWN, SCALING
    "lastUpdate": "timestamp"
  },
  "createdAt": "timestamp",
  "updatedAt": "timestamp",
  "stableTime": {                  // Per-component stability tracking
    "componentName": "timestamp"
  }
}
```

**DeploymentUnit** (Input from Package Manager)
```json
{
  "id": "string",                  // Deployment unit ID (TSID)
  "clsKey": "string",              // Target class key
  "functions": [                   // Included functions with images
    {
      "key": "string",             // Function key
      "image": "string",           // Container image
      "config": "object"           // Function-specific config
    }
  ],
  "partitionInfo": {               // Partition assignment from PM
    "partitionId": "int",          // Partition identifier
    "replicaId": "int"             // Replica identifier
  },
  "targetEnv": "string",           // Target environment
  "oPackage": "OPackage",          // Complete package definition
  "oClass": "OClass",              // Target class definition
  "oClassDeployment": "OClassDeployment", // Deployment configuration
  "nfrRequirements": {             // Extracted NFR specifications
    "performance": {
      "throughput": "int",         // From OFunction.requirements
      "latency": "int",            // From OFunction.requirements
      "availability": "float"      // From OFunction.requirements
    },
    "constraints": {
      "hardLatency": "int",        // From OFunction.constraints
      "maxConcurrency": "int"      // From OFunction.constraints
    }
  }
}
```

#### 1.2 Persistence Using Kubernetes CRDs

The CRM uses Kubernetes Custom Resource Definitions (CRDs) as the primary persistence layer:

**Benefits:**
- Native Kubernetes integration with existing cluster infrastructure
- Built-in RBAC and security through Kubernetes API
- Automatic backup and recovery through etcd
- Event-driven updates via Kubernetes watch API
- No additional database infrastructure dependencies
- Strong consistency guarantees from etcd consensus

**Core CRDs:**
- `DeploymentRecord` - Stores complete deployment state and configuration
- `DeploymentTemplate` - Manages deployment templates and selection criteria
- `EnvironmentConfig` - Stores environment-specific configurations and capabilities

### 2. Deployment Controller Manager

#### 2.1 Core Functionality

**Lifecycle Management:**
- **Accept DeploymentUnits**: Receive deployment requests from Package Manager via REST/gRPC API
- **Template Selection**: Analyze NFR requirements and select optimal deployment template
- **Resource Provisioning**: Create Kubernetes resources (Deployments, Services, ConfigMaps) based on template
- **Status Monitoring**: Track deployment progress and health status
- **State Persistence**: Store deployment state in local CRDs
- **Cleanup Operations**: Handle deployment deletion and resource cleanup

**Deployment Process:**
1. Receive `DeploymentUnit` from Package Manager
2. Extract NFR requirements from `OFunction.requirements` and `OFunction.constraints`
3. Select appropriate template based on NFR analysis
4. Generate deployment plan with resource specifications
5. Create Kubernetes resources in target environment
6. Monitor deployment status and update `DeploymentRecord`
7. Report status back to Package Manager

**State Management:**
- **Local Cache**: Maintain in-memory cache of active deployments for performance
- **CRD Synchronization**: Persist all deployment state to Kubernetes CRDs
- **State Recovery**: Rebuild local cache from CRDs on startup
- **Consistency**: Ensure consistency between cache and persistent storage
- **Conflict Resolution**: Handle concurrent updates and state conflicts

#### 2.2 Behavior Patterns

**Controller Lifecycle:**
1. **Initialization**: Load deployment templates and environment configurations from CRDs
2. **Cache Population**: Populate local cache with active deployments from CRDs
3. **Watch Setup**: Establish Kubernetes watch for CRD changes
4. **Request Handling**: Process deployment requests asynchronously
5. **Reconciliation**: Periodically reconcile actual vs desired state with Kubernetes
6. **Status Reporting**: Continuously report deployment status to Package Manager

**Error Handling:**
- **Retry Logic**: Implement exponential backoff for transient failures
- **Circuit Breaker**: Prevent cascading failures during outages
- **Partial Deployment**: Handle partial deployment failures gracefully
- **Rollback**: Support deployment rollback on critical failures
- **Alert Generation**: Generate alerts for persistent failures

### 3. Template Management System

#### 3.1 Template Types and NFR Enforcement

**Knative Template:**
- **Purpose**: Serverless function deployment with auto-scaling
- **Optimized For**: Event-driven workloads, cost efficiency, burst handling
- **NFR Capabilities**:
  - Low baseline resource usage (scale to zero)
  - Rapid scaling for throughput bursts
  - Cost optimization through pay-per-use model
  - Cold start latency trade-offs
- **Best For**: Functions with `throughput` < 1000 RPS, bursty traffic patterns, cost-sensitive workloads

**Kubernetes Deployment Template:**
- **Purpose**: Traditional container deployment with persistent instances
- **Optimized For**: Consistent performance, predictable latency, high availability
- **NFR Capabilities**:
  - Consistent response times (no cold starts)
  - High throughput capacity
  - Predictable resource allocation
  - Always-on availability
- **Best For**: Functions with `throughput` > 1000 RPS, strict `latency` requirements, steady workloads

**Unified Template:**
- **Purpose**: Hybrid approach combining serverless and traditional deployment
- **Optimized For**: Mixed workload patterns, adaptive performance
- **NFR Capabilities**:
  - Dynamic scaling based on traffic patterns
  - Balance between cost and performance
  - Adaptive resource allocation
  - Flexible deployment strategies
- **Best For**: Functions with variable traffic patterns, moderate NFR requirements

#### 3.2 NFR-Based Template Selection

**NFR Extraction from DeploymentUnit:**

The CRM analyzes the complete `DeploymentUnit` to extract NFR specifications:

1. **Function Requirements** (from `OFunction.requirements`):
   - `throughput`: Target requests per second
   - `latency`: Maximum acceptable response time (ms)
   - `availability`: Availability percentage (handled by PM replication)

2. **Function Constraints** (from `OFunction.constraints`):
   - `hardLatency`: Hard latency limit that must never be exceeded (ms)
   - `maxConcurrency`: Maximum concurrent executions allowed

3. **Provision Specifications** (from `OFunction.provision`):
   - `minScale`/`maxScale`: Auto-scaling boundaries
   - `cpu`/`memory`: Resource allocation requirements
   - Environment-specific configurations

4. **Deployment Overrides** (from `OClassDeployment.members`):
   - `forceTemplate`: Explicit template selection override
   - `standbyFn`: Functions designated for cost-optimized deployment
   - `disabledFn`: Functions to exclude from deployment

**Template Selection Algorithm:**
1. **Constraint Validation**: Check if `hardLatency` constraint can be met by any template
2. **Override Check**: Apply `forceTemplate` if specified in deployment configuration
3. **NFR Scoring**: Score each template against NFR requirements:
   - **Performance Score**: Match template capabilities with `throughput` and `latency` requirements
   - **Scalability Score**: Evaluate auto-scaling alignment with `minScale`/`maxScale`
   - **Cost Score**: Consider `standbyFn` designation and resource efficiency
4. **Template Selection**: Choose template with highest composite NFR score
5. **Validation**: Ensure selected template can meet all hard constraints
6. **Fallback**: Reject deployment if no template can satisfy NFR requirements

**Template Scoring Matrix:**

| NFR Requirement | Knative Template | K8s Deployment | Unified Template |
|-----------------|------------------|----------------|------------------|
| `throughput` < 100 RPS | High (9/10) | Medium (6/10) | High (8/10) |
| `throughput` > 1000 RPS | Low (4/10) | High (9/10) | Medium (7/10) |
| `latency` < 50ms | Low (3/10) | High (9/10) | Medium (6/10) |
| `latency` < 500ms | High (8/10) | High (9/10) | High (8/10) |
| Cost Optimization | High (9/10) | Low (3/10) | Medium (6/10) |
| Burst Traffic | High (9/10) | Medium (5/10) | High (8/10) |
| Steady Traffic | Medium (5/10) | High (9/10) | High (7/10) |

#### 3.3 NFR Validation and Enforcement

**Validation Process:**
1. **Extract NFRs**: Parse requirements and constraints from `DeploymentUnit`
2. **Environment Capability Check**: Validate environment can support NFR requirements
3. **Resource Feasibility**: Verify requested resources are within environment quotas
4. **Constraint Validation**: Ensure `hardLatency` and `maxConcurrency` can be guaranteed
5. **Template Compatibility**: Confirm selected template can meet all NFR specifications
6. **Deployment Configuration**: Apply NFR-specific configurations to selected template

**Enforcement Mechanisms:**
- **Resource Limits**: Set Kubernetes resource requests/limits based on `OFunction.provision`
- **Auto-scaling**: Configure HPA/VPA based on `minScale`/`maxScale` and `throughput` requirements
- **Health Checks**: Set probe timeouts based on `latency` requirements and `timeout` configuration
- **Circuit Breakers**: Implement circuit breakers for `maxConcurrency` constraint enforcement
- **Performance Monitoring**: Track actual performance against `throughput` and `latency` requirements
- **Alerting**: Generate alerts for NFR violations or constraint breaches

**NFR Violation Handling:**
- **Hard Constraint Violations**: Immediately reject deployment if `hardLatency` cannot be guaranteed
- **Performance Degradation**: Auto-scale resources if `throughput` requirements are not met
- **Latency Violations**: Implement circuit breakers and alert for consistent `latency` violations
- **Capacity Limits**: Queue deployments if `maxConcurrency` would be exceeded
- **Resource Exhaustion**: Reject new deployments if environment quotas would be exceeded

### 4. Environment Management

#### 4.1 Environment Configuration Schema

```json
{
  "environments": {
    "dev": {
      "namespace": "oaas-dev",
      "apiServer": "https://dev-k8s-api.example.com",
      "kubeconfig": "/etc/kubeconfig/dev",
      "resources": {
        "defaultCpuLimit": "1000m",
        "defaultMemoryLimit": "512Mi",
        "quotas": {
          "cpu": "50",
          "memory": "100Gi",
          "pods": "500",
          "persistentVolumes": "50",
          "services": "100"
        }
      },
      "features": {
        "knativeEnabled": true,
        "istioEnabled": false,
        "monitoringEnabled": true,
        "networkPoliciesEnabled": true
      },
      "security": {
        "podSecurityStandards": "restricted",
        "rbacEnabled": true,
        "admissionControllers": ["PodSecurity", "ResourceQuota"]
      },
      "templates": {
        "allowed": ["knative", "k8s-deployment", "unified"],
        "default": "knative",
        "restrictions": {
          "maxCpu": "2000m",
          "maxMemory": "4Gi",
          "maxReplicas": 10
        }
      }
    },
    "prod": {
      "namespace": "oaas-prod",
      "apiServer": "https://prod-k8s-api.example.com",
      "kubeconfig": "/etc/kubeconfig/prod",
      "resources": {
        "defaultCpuLimit": "2000m",
        "defaultMemoryLimit": "1Gi",
        "quotas": {
          "cpu": "1000",
          "memory": "2000Gi",
          "pods": "5000",
          "persistentVolumes": "500",
          "services": "1000"
        }
      },
      "features": {
        "knativeEnabled": true,
        "istioEnabled": true,
        "monitoringEnabled": true,
        "networkPoliciesEnabled": true
      },
      "security": {
        "podSecurityStandards": "restricted",
        "rbacEnabled": true,
        "admissionControllers": ["PodSecurity", "ResourceQuota", "NetworkPolicy"]
      },
      "templates": {
        "allowed": ["knative", "k8s-deployment", "unified"],
        "default": "unified",
        "restrictions": {
          "maxCpu": "8000m",
          "maxMemory": "16Gi",
          "maxReplicas": 100
        }
      }
    }
  },
  "managedEnvironments": ["dev", "prod"]
}
```

#### 4.2 Environment Management Behavior

**Environment Discovery and Validation:**
- **Configuration Loading**: Load environment configurations from CRDs on startup
- **Connectivity Testing**: Validate Kubernetes API server connectivity for each environment
- **Feature Detection**: Detect available features (Knative, Istio, monitoring) in each environment
- **Quota Validation**: Verify resource quotas and available capacity
- **Template Compatibility**: Validate which templates are supported in each environment

**Resource Management:**
- **Quota Enforcement**: Ensure deployments stay within environment resource quotas
- **Resource Monitoring**: Track resource usage across all deployments in environment
- **Capacity Planning**: Alert when environment approaches resource limits
- **Load Balancing**: Distribute deployments across available nodes and zones

**Environment Health Monitoring:**
- **API Server Health**: Monitor Kubernetes API server availability and response times
- **Node Health**: Track node status and resource availability
- **Network Connectivity**: Verify inter-service communication and external connectivity
- **Feature Status**: Monitor status of environment features (Knative, Istio, etc.)

### 5. Communication with Package Manager

#### 5.1 API Integration

**REST API Endpoints (CRM receives from PM):**
```
POST /api/v1/deploy
- Accept DeploymentUnit for processing
- Validate NFR requirements
- Return deployment acknowledgment

PUT /api/v1/deployments/{id}/status
- Update deployment status
- Report NFR compliance metrics
- Return updated status

GET /api/v1/deployments/{id}
- Retrieve deployment details
- Return current status and metrics

DELETE /api/v1/deployments/{id}
- Initiate deployment cleanup
- Remove all associated resources
- Return cleanup status
```

**Status Reporting to Package Manager:**
```json
{
  "deploymentId": "string",
  "status": "PENDING|DEPLOYING|RUNNING|DOWN|DELETED",
  "environment": "string",
  "functionStatuses": {
    "functionKey": {
      "condition": "PENDING|DEPLOYING|RUNNING|DOWN|DELETED",
      "invocationUrl": "string",
      "errorMsg": "string",
      "nfrCompliance": {
        "throughputActual": "int",
        "latencyP95": "int",
        "availabilityActual": "float"
      }
    }
  },
  "lastUpdated": "timestamp"
}
```

#### 5.2 Deployment Flow Integration

**Deployment Request Flow:**
1. **Package Manager** creates `DeploymentUnit` from `OPackage`, `OClass`, and `OClassDeployment`
2. **Package Manager** sends `DeploymentUnit` to CRM via REST API
3. **CRM** validates NFR requirements and selects appropriate template
4. **CRM** creates Kubernetes resources and updates local `DeploymentRecord` CRD
5. **CRM** monitors deployment progress and reports status back to Package Manager
6. **Package Manager** updates `OFunction.status` based on CRM reports

**State Synchronization:**
- **Deployment Creation**: CRM creates local `DeploymentRecord` when accepting `DeploymentUnit`
- **Status Updates**: CRM continuously reports deployment status to Package Manager
- **Function URLs**: CRM provides invocation URLs for deployed functions
- **Error Reporting**: CRM reports detailed error messages for failed deployments
- **Metrics Reporting**: CRM reports NFR compliance metrics for monitoring

### 6. Monitoring and Observability

#### 6.1 Metrics Collection and Reporting

**Deployment Metrics:**
- `crm_deployments_total{environment, template, status}` - Total deployments by status
- `crm_deployment_duration_seconds{environment, template}` - Deployment duration histogram
- `crm_deployment_failures_total{environment, reason}` - Deployment failures by reason
- `crm_active_deployments{environment}` - Currently active deployments

**NFR Compliance Metrics:**
- `crm_function_throughput{function_key, environment}` - Actual function throughput
- `crm_function_latency_seconds{function_key, environment, quantile}` - Function latency percentiles
- `crm_nfr_violations_total{function_key, environment, type}` - NFR violations by type
- `crm_constraint_breaches_total{function_key, environment, constraint}` - Hard constraint breaches

**Resource Utilization Metrics:**
- `crm_resource_usage{environment, resource_type}` - CPU, memory usage by environment
- `crm_quota_utilization{environment, quota_type}` - Resource quota utilization percentage
- `crm_template_usage{template, environment}` - Template usage distribution

#### 6.2 Health Status Schema

```json
{
  "status": "HEALTHY|DEGRADED|UNHEALTHY",
  "timestamp": "ISO8601",
  "version": "string",
  "environment": "string",
  "checks": {
    "kubernetesApi": {
      "status": "HEALTHY|DEGRADED|UNHEALTHY",
      "message": "Kubernetes API connectivity status",
      "lastChecked": "ISO8601",
      "responseTime": "duration"
    },
    "crdStorage": {
      "status": "HEALTHY|DEGRADED|UNHEALTHY",
      "message": "CRD storage accessibility",
      "lastChecked": "ISO8601",
      "recordCount": "int"
    },
    "templateValidation": {
      "status": "HEALTHY|DEGRADED|UNHEALTHY",
      "message": "Deployment template validation",
      "lastChecked": "ISO8601",
      "validTemplates": ["string"]
    },
    "environmentConnectivity": {
      "status": "HEALTHY|DEGRADED|UNHEALTHY",
      "message": "Multi-environment connectivity",
      "lastChecked": "ISO8601",
      "environments": {
        "dev": "CONNECTED|DISCONNECTED|UNKNOWN",
        "prod": "CONNECTED|DISCONNECTED|UNKNOWN"
      }
    }
  },
  "metrics": {
    "activeDeployments": "int",
    "totalDeployments": "int",
    "failedDeployments": "int",
    "nfrViolations": "int"
  }
}
```
### 7. Error Handling and Recovery

#### 7.1 Error Categories and Handling Strategies

**Deployment Errors:**
- **NFR Constraint Violations**: Reject deployments that cannot meet `hardLatency` or `maxConcurrency` constraints
- **Resource Quota Exceeded**: Queue or reject deployments when environment quotas would be exceeded
- **Template Incompatibility**: Select alternative template or reject if no template can satisfy NFRs
- **Image Pull Failures**: Retry with exponential backoff, report detailed error to Package Manager
- **Configuration Errors**: Validate all configurations before deployment, provide detailed error messages

**Infrastructure Errors:**
- **Kubernetes API Failures**: Implement circuit breaker pattern, maintain local cache during outages
- **Network Connectivity Issues**: Retry operations with exponential backoff, switch to degraded mode
- **CRD Storage Failures**: Queue operations in memory, replay when storage recovers
- **Node Resource Exhaustion**: Redistribute deployments, trigger node scaling if available

**Application Errors:**
- **Function Deployment Failures**: Monitor deployment status, trigger rollback on persistent failures
- **Health Check Failures**: Restart unhealthy instances, report to Package Manager
- **Performance Degradation**: Auto-scale resources, alert if NFR violations persist
- **Memory/CPU Limits Exceeded**: Adjust resource allocations, consider template migration

#### 7.2 Recovery Mechanisms

**State Recovery:**
- **CRD Recovery**: Rebuild local cache from CRDs on startup or after connectivity loss
- **Deployment Reconciliation**: Compare desired vs actual state, trigger corrective actions
- **Status Synchronization**: Re-sync deployment status with Package Manager after recovery
- **Resource Cleanup**: Clean up orphaned resources from failed deployments

**Graceful Degradation:**
- **Read-Only Mode**: Continue serving status queries when write operations fail
- **Local Cache Mode**: Serve from local cache when CRD storage is unavailable
- **Best-Effort Deployment**: Deploy with reduced NFR guarantees during resource constraints
- **Priority-Based Queuing**: Prioritize critical deployments during resource shortage

### 8. Configuration Management

#### 8.1 CRM Configuration Schema

```yaml
crm:
  # Core Service Configuration
  server:
    host: "0.0.0.0"
    port: 8080
    shutdownTimeout: "30s"

  # Kubernetes Integration
  kubernetes:
    namespace: "oaas-system"
    crdGroup: "crm.oaas.io"
    crdVersion: "v1"
    watchTimeout: "300s"

  # Environment Management
  environments:
    managed: ["dev", "staging", "prod"]
    configPath: "./environments.yaml"
    refreshInterval: "60s"
    healthCheckInterval: "30s"

  # Template Configuration
  templates:
    loadOnStart: true
    path: "./templates"
    defaultTemplate: "unified"
    selectionTimeout: "10s"

  # Package Manager Integration
  packageManager:
    baseUrl: "http://package-manager:8080"
    timeout: "30s"
    retryAttempts: 3
    retryBackoff: "1s"
    statusReportInterval: "10s"

  # Monitoring and Observability
  monitoring:
    enabled: true
    metricsPath: "/metrics"
    metricsInterval: "15s"
    healthPath: "/health"

  # Performance and Resource Management
  performance:
    maxConcurrentDeployments: 50
    deploymentTimeout: "300s"
    cacheSize: 1000
    cacheTTL: "3600s"

  # Logging Configuration
  logging:
    level: "INFO"
    format: "json"
    auditEnabled: true
    auditPath: "./logs/audit.log"
```

#### 8.2 Environment-Specific Overrides

```yaml
# Environment-specific configuration overrides
environments:
  dev:
    performance:
      maxConcurrentDeployments: 10
      deploymentTimeout: "600s"  # Longer timeout for dev
    logging:
      level: "DEBUG"

  prod:
    performance:
      maxConcurrentDeployments: 100
      deploymentTimeout: "180s"  # Shorter timeout for prod
    monitoring:
      metricsInterval: "5s"      # More frequent metrics
    logging:
      level: "WARN"
```

### 9. API Specifications

#### 9.1 REST API Endpoints

**Deployment Management:**
```
POST /api/v1/deployments
- Description: Create new deployment from DeploymentUnit
- Content-Type: application/json
- Body: DeploymentUnit object
- Response: 201 Created with deployment ID
- Error Responses: 400 (Invalid NFR), 409 (Resource conflict), 503 (Service unavailable)

GET /api/v1/deployments
- Description: List all deployments with optional filtering
- Query Parameters: environment, status, packageName
- Response: 200 OK with deployment list
- Supports pagination via limit/offset parameters

GET /api/v1/deployments/{id}
- Description: Get specific deployment details
- Response: 200 OK with complete DeploymentRecord
- Error Responses: 404 (Not found)

PUT /api/v1/deployments/{id}
- Description: Update deployment configuration
- Body: Partial DeploymentRecord with updates
- Response: 200 OK with updated deployment
- Error Responses: 400 (Invalid update), 404 (Not found), 409 (Conflict)

DELETE /api/v1/deployments/{id}
- Description: Delete deployment and cleanup resources
- Response: 202 Accepted (async deletion)
- Error Responses: 404 (Not found), 409 (Deletion in progress)
```

**Environment Management:**
```
GET /api/v1/environments
- Description: List managed environments with status
- Response: 200 OK with environment list and health status

GET /api/v1/environments/{env}/deployments
- Description: List deployments in specific environment
- Response: 200 OK with filtered deployment list

POST /api/v1/environments/{env}/sync
- Description: Trigger environment synchronization
- Response: 202 Accepted (async operation)
```

**Template Management:**
```
GET /api/v1/templates
- Description: List available deployment templates
- Response: 200 OK with template list and capabilities

GET /api/v1/templates/{name}
- Description: Get specific template details
- Response: 200 OK with template configuration

POST /api/v1/templates/{name}/validate
- Description: Validate template against NFR requirements
- Body: NFR requirements object
- Response: 200 OK with validation result and score
```

#### 9.2 Status and Health Endpoints

```
GET /health
- Description: Overall health status
- Response: 200 OK (healthy), 503 Service Unavailable (unhealthy)

GET /health/ready
- Description: Readiness probe for Kubernetes
- Response: 200 OK (ready to serve traffic)

GET /health/live
- Description: Liveness probe for Kubernetes
- Response: 200 OK (process is alive)

GET /metrics
- Description: Prometheus metrics endpoint
- Response: 200 OK with metrics in Prometheus format

GET /api/v1/status
- Description: Detailed system status including metrics
- Response: 200 OK with comprehensive status object
```


## Conclusion

This specification provides a complete blueprint for implementing a self-contained Class Runtime Manager that:

1. **Manages Local Deployment Data** using Kubernetes CRDs for persistence
2. **Enforces Non-Functional Requirements** through intelligent template selection and configuration
3. **Operates Independently** from Package Manager data services while maintaining integration
4. **Supports Multi-Environment Deployments** with environment-specific configurations and quotas
5. **Provides Comprehensive Monitoring** with metrics, health checks, and observability
6. **Handles Failures Gracefully** with robust error handling and recovery mechanisms

The specification focuses on functionality, behavior, and data schemas while remaining language-agnostic, allowing implementation teams to choose appropriate technologies based on their expertise and requirements. The NFR enforcement capability ensures that deployed functions meet their performance, scalability, and cost requirements while maintaining system reliability and observability.
