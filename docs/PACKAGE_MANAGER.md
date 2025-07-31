# Package Manager - OaaS Control Plane

## Overview

The Package Manager is the central control plane component of the Object-as-a-Service (OaaS) serverless platform. It serves as the orchestration layer responsible for accepting packages, managing deployments, and coordinating with registered Class Runtime Managers (CRMs) to deploy and manage serverless functions and classes across the platform.

## Data Schema

The Package Manager operates with several core data models that define the structure of packages, classes, functions, and deployments:

### OPackage
The root container for OaaS deployments, representing a complete application package.

```json
{
  "name": "string",           // Package identifier
  "classes": [OClass],        // Array of class definitions
  "functions": [OFunction],   // Array of function definitions
  "required": ["string"],     // Dependencies on other packages
  "deployments": [OClassDeployment], // Deployment configurations
  "disabled": false           // Package availability flag
}
```

### OClass
Defines object classes with their state specifications and function bindings.

```json
{
  "_key": "string",           // Unique class identifier
  "_rev": "string",           // Internal revision (system managed)
  "name": "string",           // Human-readable class name
  "pkg": "string",            // Parent package name
  "stateType": "enum",        // NORMAL, COLLECTION, SINGLETON
  "functions": [FunctionBinding], // Bound functions for this class
  "stateSpec": {              // State structure definition
    "keySpecs": [KeySpecification],
    "defaultProvider": "string"
  },
  "refSpec": [ReferenceSpecification], // References to other classes
  "parents": ["string"],      // Inheritance hierarchy
  "description": "string",    // Documentation
  "disabled": false,          // Availability flag
  "markForRemoval": false     // Deletion flag
}
```

### OFunction
Represents serverless functions with their execution specifications.

```json
{
  "_key": "string",           // Unique function identifier
  "name": "string",           // Function name
  "pkg": "string",            // Parent package
  "description": "string",    // Documentation
  "type": "enum",             // BUILTIN, TASK, MACRO, LOGICAL
  "outputCls": "string",      // Output class type
  "macro": {                  // Macro function specification
    "steps": [DataflowStep]
  },
  "provision": {              // Runtime provisioning config
    "knative": {
      "minScale": 0,
      "maxScale": 100,
      "cpu": "100m",
      "memory": "128Mi"
    }
  },
  "variableDescriptions": [VariableDescription],
  "status": {                 // Deployment status
    "condition": "enum",      // PENDING, DEPLOYING, RUNNING, DOWN, DELETED
    "invocationUrl": "string",
    "errorMsg": "string"
  },
  "state": "enum",            // ENABLED, DISABLED, FROZEN
  "requirements": {           // QoS requirements
    "throughput": 1000,
    "latency": 100
  },
  "constraints": {            // QoS constraints
    "hardLatency": 500
  },
  "config": {                 // Function-specific configuration
    "timeout": 30000,
    "env": {"key": "value"}
  },
  "immutable": false          // Immutability flag
}
```

### OClassDeployment
Defines deployment configurations for classes across environments.

```json
{
  "_key": "string",           // Deployment identifier
  "partitionCount": 1,        // Number of data partitions
  "replicaCount": 1,          // Number of replicas per partition
  "targetEnvs": ["string"],   // Target environment names
  "members": [                // Member group configurations
    {
      "id": 1,
      "env": "string",        // Environment name
      "maxShards": -1,        // Maximum shards (-1 = unlimited)
      "allPartitions": false, // Handle all partitions flag
      "disabledFn": ["string"], // Disabled functions
      "standbyFn": ["string"],  // Standby functions
      "forceTemplate": "string" // Forced template name
    }
  ],
  "shardType": "basic",       // Sharding strategy
  "assignments": [            // Shard assignments
    {
      "primary": 1,           // Primary member ID
      "owners": [1, 2],       // Owner member IDs
      "shardIds": [0, 1]      // Assigned shard IDs
    }
  ],
  "options": {                // Additional deployment options
    "key": "value"
  }
}
```

### FunctionBinding
Links functions to classes with access control and routing configuration.

```json
{
  "name": "string",           // Binding name
  "function": "string",       // Function key reference
  "access": "enum",           // PUBLIC, INTERNAL, PRIVATE
  "defaultRoute": true,       // Default routing flag
  "condition": "string",      // Conditional binding expression
  "immutable": false          // Immutability flag
}
```

### StateSpecification
Defines the structure and storage of object state.

```json
{
  "keySpecs": [               // Key specifications
    {
      "name": "string",       // Key name
      "type": "enum",         // STRING, INTEGER, DOUBLE, BOOLEAN
      "optional": false       // Optional flag
    }
  ],
  "defaultProvider": "string" // Default storage provider
}
```

### ProvisionConfig
Runtime provisioning configuration for functions.

```json
{
  "knative": {                // Knative-specific config
    "minScale": 0,            // Minimum replicas
    "maxScale": 100,          // Maximum replicas
    "cpu": "100m",            // CPU allocation
    "memory": "128Mi",        // Memory allocation
    "env": {                  // Environment variables
      "key": "value"
    },
    "requestsCpu": "50m",     // CPU requests
    "requestsMemory": "64Mi", // Memory requests
    "image": "string",        // Container image
    "pullPolicy": "IfNotPresent"
  },
  "docker": {                 // Docker-specific config
    "image": "string",
    "port": 8080,
    "env": {"key": "value"}
  }
}
```

### QoS Specifications

#### QosRequirement
Performance requirements for functions.

```json
{
  "throughput": 1000,         // Requests per second
  "latency": 100,             // Maximum latency (ms)
  "availability": 0.99        // Availability percentage
}
```

#### QosConstraint
Hard constraints for function execution.

```json
{
  "hardLatency": 500,         // Hard latency limit (ms)
  "maxConcurrency": 50        // Maximum concurrent executions
}
```

### Runtime State Models

#### OClassRuntime
Runtime state of a deployed class.

```json
{
  "_key": "string",           // Runtime identifier
  "clsKey": "string",         // Associated class key
  "env": "string",            // Environment name
  "partitions": [             // Partition information
    {
      "id": 0,
      "replicas": ["node1", "node2"]
    }
  ],
  "status": "enum",           // RUNNING, DOWN, SCALING
  "lastUpdate": "timestamp"
}
```

#### DeploymentUnit
Unit of deployment sent to Class Runtime Managers.

```json
{
  "id": "string",             // Deployment unit ID
  "clsKey": "string",         // Target class
  "functions": [              // Included functions
    {
      "key": "string",
      "image": "string",
      "config": {}
    }
  ],
  "partitionInfo": {          // Partition assignment
    "partitionId": 0,
    "replicaId": 1
  },
  "targetEnv": "string"       // Target environment
}
```

### Enumerations

#### StateType
- `NORMAL`: Regular object with mutable state
- `COLLECTION`: Collection of objects
- `SINGLETON`: Single instance object

#### FunctionType
- `BUILTIN`: System-provided function
- `TASK`: User-defined task function
- `MACRO`: Workflow composition
- `LOGICAL`: Logic-only function

#### DeploymentCondition
- `PENDING`: Awaiting deployment
- `DEPLOYING`: Currently deploying
- `RUNNING`: Successfully deployed and running
- `DOWN`: Deployment failed or stopped
- `DELETED`: Deployment removed

#### FunctionAccessModifier
- `PUBLIC`: Accessible from external clients
- `INTERNAL`: Accessible within the same package
- `PRIVATE`: Accessible only within the same class

#### ConsistencyModel
- `NONE`: No consistency guarantees
- `READ_YOUR_WRITE`: Read-your-write consistency
- `BOUNDED_STALENESS`: Bounded staleness consistency
- `STRONG`: Strong consistency

This schema provides the foundation for all package management operations, ensuring type safety and validation across the OaaS platform.

## Architecture

The Package Manager follows a microservices architecture with the following key components:

### Core Components

1. **Package Management Service**
   - Handles package creation and validation
   - Supports both JSON and YAML package formats
   - Manages the lifecycle of OaaS packages
   - Provides RESTful API endpoints for package operations

2. **Class Deployment Manager**
   - Orchestrates class deployments across environments
   - Manages replica distribution and availability targets
   - Handles deployment state transitions
   - Calculates optimal resource allocation

3. **Class Runtime State Manager**
   - Maintains state of Class Runtime instances
   - Broadcasts runtime state changes
   - Manages hash-based consistency for deployments
   - Tracks deployment health and status

4. **Environment Registry**
   - Maintains registry of available deployment environments
   - Tracks Class Runtime Manager instances
   - Handles environment-specific configurations
   - Manages environment capacity and availability

5. **Package Publisher**
   - Publishes deployment events via message queue
   - Handles asynchronous communication with runtime managers
   - Manages event-driven deployment workflows

### Communication Patterns

- **REST API**: Synchronous client interactions
- **Message Queue**: Asynchronous inter-service communication
- **gRPC** (optional): High-performance service-to-service calls
- **WebSocket** (optional): Real-time state updates

## Key Features

### Package Management
- **Multi-format Support**: Accepts packages in both JSON and YAML formats
- **Package Validation**: Comprehensive validation of package structure and dependencies
- **Class Resolution**: Resolves class dependencies and function bindings
- **Version Management**: Handles package versioning and updates

### Deployment Orchestration
- **Multi-environment Deployment**: Supports deployment across multiple environments
- **Replica Management**: Calculates and manages optimal replica counts based on availability targets
- **Load Distribution**: Distributes workloads across available Class Runtime Managers
- **Deployment State Tracking**: Monitors deployment status (PENDING, DEPLOYING, RUNNING, DOWN, DELETED)

### High Availability
- **Availability Target Calculation**: Automatically calculates minimum replicas based on availability requirements
- **Fault Tolerance**: Handles Class Runtime Manager failures gracefully
- **State Consistency**: Maintains consistent state across distributed components

## API Endpoints

### Package Operations
```
POST /api/packages
- Content-Type: application/json | text/x-yaml
- Creates a new package deployment
- Returns: Package metadata with deployment status
```

### Class Management
```
GET /classes
- Lists all deployed classes with pagination support

GET /classes/{clsKey}
- Retrieves specific class information

DELETE /classes/{clsKey}
- Removes a class deployment
```

### Function Management
```
GET /functions
- Lists all deployed functions

GET /functions/{fnKey}
- Retrieves specific function information
```

### Runtime Management
```
GET /cr
- Lists Class Runtime instances

POST /cr/{crId}/update
- Updates Class Runtime state
```

### Deployment Management
```
GET /deployments
- Lists all deployments with status

POST /deployments
- Creates new deployment

PUT /deployments/{id}
- Updates deployment configuration
```

## Data Flow

1. **Package Submission**
   - User submits package (JSON/YAML) via REST API
   - Package Manager validates package structure
   - Dependencies are resolved and validated

2. **Deployment Planning**
   - Class Deployment Manager calculates deployment requirements
   - Availability targets determine replica counts
   - Environment Registry selects suitable Class Runtime Managers

3. **Runtime Deployment**
   - Deployment units are created and distributed
   - Package Publisher sends deployment commands via Kafka
   - Class Runtime Managers receive and execute deployments

4. **State Management**
   - Runtime state updates are received from Class Runtime Managers
   - CrStateManager maintains current state and broadcasts changes
   - Health monitoring ensures deployment integrity

## Configuration

### Essential Configuration Properties
- **Runtime Integration**: Enable/disable Class Runtime Manager integration
- **Environment Settings**: Configuration for different deployment targets
- **Message Queue**: Configuration for asynchronous communication
- **Database**: Configuration for state persistence
- **Security**: Authentication and authorization settings
- **Resource Limits**: Memory, CPU, and storage constraints
- **Retry Policies**: Deployment retry and backoff strategies

### Environment Variables
```
# Core Configuration
OAAS_PM_CRM_ENABLED=true
OAAS_PM_PORT=8080
OAAS_PM_LOG_LEVEL=INFO

# Database Configuration
OAAS_PM_DB_HOST=localhost
OAAS_PM_DB_PORT=8529
OAAS_PM_DB_NAME=oaas_pm
OAAS_PM_DB_USERNAME=admin
OAAS_PM_DB_PASSWORD=secret

# Message Queue Configuration
OAAS_PM_MQ_BROKERS=localhost:9092
OAAS_PM_MQ_TOPIC_PREFIX=oaas.pm
OAAS_PM_MQ_GROUP_ID=package-manager

# Monitoring Configuration
OAAS_PM_METRICS_ENDPOINT=/metrics
OAAS_PM_HEALTH_ENDPOINT=/health
```

## Data Storage Alternatives

The Package Manager requires persistent storage for metadata and state management. Here are recommended alternatives to ArangoDB:

### Availability-First Database Recommendations
#### 3. etcd (Distributed Key-Value Store)
**Best for:** Configuration management, service discovery, distributed coordination

**Availability Features:**
- Raft consensus algorithm with strong consistency and leader election
- Automatic failover with leader re-election on node failures
- Watch API for real-time change notifications
- Multi-node clusters with quorum-based decisions

**Architectural Considerations:**
- Data model: Hierarchical key-value store (not document-based)
- Storage limit: Recommended maximum 8GB per cluster
- Query capabilities: Key-based access, prefix queries, range queries only
- Transaction support: Multi-key transactions with compare-and-swap

**Deployment Example:**
- Cluster size: 3 nodes
- Quorum: 2 nodes
- Leader election timeout: 1000ms
- Heartbeat interval: 100ms

**⚠️ Architecture Redesign Implications for etcd:**

**Data Modeling Changes:**
- From: Document-based JSON storage
- To: Hierarchical key-value structure
- Impact: Significant data model restructuring required

**Examples:**
- Packages:
  - Current: Single JSON document per package
  - etcd: Multiple keys like /packages/{name}/metadata, /packages/{name}/classes/*, /packages/{name}/functions/*
- Queries:
  - Current: Rich JSON queries and aggregations
  - etcd: Key-based lookups and prefix scans only

**Required Redesigns:**

**API Layer (Medium Complexity):**
- API endpoints need to aggregate data from multiple etcd keys
- Requires data composition logic

**Query Patterns (High Complexity):**
- Replace complex queries with key-based access patterns
- Fundamental query redesign needed

**Data Relationships (Medium Complexity):**
- Implement relationships through key hierarchies
- Use key prefixes for associations

**Caching Strategy (Medium Complexity):**
- More aggressive caching needed due to limited query capabilities
- Implement application-level caching

**Transaction Handling (High Complexity):**
- Redesign multi-document operations as multi-key transactions
- Careful transaction boundary design needed

**Recommended etcd Data Structure:**

**Packages:**
- Key pattern: /packages/{package_name}
- Value: Package metadata only

**Classes:**
- Key pattern: /packages/{package_name}/classes/{class_key}
- Value: Individual class definition

**Functions:**
- Key pattern: /packages/{package_name}/functions/{function_key}
- Value: Individual function definition

**Deployments:**
- Key pattern: /deployments/{class_key}/{environment}
- Value: Deployment configuration

**Runtime State:**
- Key pattern: /runtime/{environment}/{class_key}
- Value: Runtime state information

**Indexes:**
- By package: /indexes/by_package/{package_name} -> list of class_keys
- By environment: /indexes/by_environment/{env} -> list of deployment_keys
- By status: /indexes/by_status/{status} -> list of deployment_keys

