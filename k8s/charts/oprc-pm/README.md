# OaaS Package Manager (PM) Helm Chart

This Helm chart deploys the OaaS Package Manager (PM), a service that manages OaaS packages and orchestrates class/function deployments across CRM-managed clusters.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- CRM (Class Runtime Manager) deployed and accessible

## Installing the Chart

To install the chart with the release name `my-pm`:

```bash
helm install my-pm ./oprc-pm
```

## Uninstalling the Chart

To uninstall/delete the `my-pm` deployment:

```bash
helm delete my-pm
```

The command removes all the Kubernetes components associated with the chart and deletes the release.

## Configuration

The following table lists the configurable parameters of the PM chart and their default values.

### Global Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of PM replicas | `1` |
| `image.repository` | PM image repository | `ghcr.io/pawissanutt/oaas-rs/pm` |
| `image.tag` | PM image tag | `latest` |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `nameOverride` | String to partially override the fullname | `""` |
| `fullnameOverride` | String to fully override the fullname | `""` |

### Service Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Kubernetes Service type (ClusterIP/NodePort/LoadBalancer) | `ClusterIP` |
| `service.port` | Service port | `8080` |
| `service.targetPort` | Container target port | `8080` |
| `service.nodePort` | NodePort (30000-32767, only when type=NodePort) | `30080` |

### Server Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.server.host` | Server host | `0.0.0.0` |
| `config.server.port` | Server port | `8080` |
| `config.server.workers` | Number of worker threads | `null` |

### Storage Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.storage.type` | Storage backend type (memory/etcd) | `memory` |
| `config.storage.etcd.useSubchart` | Use embedded etcd subchart | `false` |
| `config.storage.etcd.endpoints` | External etcd endpoints | `localhost:2379` |
| `config.storage.etcd.tls.insecure` | Use insecure connection | `true` |
| `config.storage.etcd.tls.caCertPath` | Path to CA certificate | `""` |
| `config.storage.etcd.tls.clientCertPath` | Path to client certificate | `""` |
| `config.storage.etcd.tls.clientKeyPath` | Path to client key | `""` |

### Etcd Subchart Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `etcd.enabled` | Enable embedded etcd deployment | `false` |
| `etcd.replicaCount` | Number of etcd replicas | `3` |
| `etcd.auth.rbac.create` | Enable RBAC authentication | `false` |
| `etcd.auth.token.enabled` | Enable token authentication | `false` |
| `etcd.persistence.enabled` | Enable persistent storage | `true` |
| `etcd.persistence.size` | Storage size for etcd | `8Gi` |
| `etcd.resources.limits.cpu` | CPU limit for etcd | `200m` |
| `etcd.resources.limits.memory` | Memory limit for etcd | `256Mi` |
| `etcd.metrics.enabled` | Enable etcd metrics | `false` |
| `etcd.autoCompactionMode` | Auto-compaction mode | `periodic` |
| `etcd.autoCompactionRetention` | Auto-compaction retention | `1h` |

### CRM Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.crm.default.url` | Default CRM cluster URL | `http://oprc-crm:8088` |
| `config.crm.default.timeout` | CRM request timeout (seconds) | `30` |
| `config.crm.default.retryAttempts` | Number of retry attempts | `3` |
| `config.crm.healthCheckInterval` | Health check interval (seconds) | `60` |
| `config.crm.healthCacheTtl` | Health cache TTL (seconds) | `15` |

### Deployment Policy

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.deployment.maxRetries` | Max deployment retries per cluster | `2` |
| `config.deployment.rollbackOnPartial` | Rollback on partial deployment failure | `false` |

### Package Management

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.package.deleteCascade` | Cascade delete deployments when deleting packages | `false` |

### Autoscaling

| Parameter | Description | Default |
|-----------|-------------|---------|
| `autoscaling.enabled` | Enable horizontal pod autoscaler | `false` |
| `autoscaling.minReplicas` | Minimum number of replicas | `1` |
| `autoscaling.maxReplicas` | Maximum number of replicas | `3` |
| `autoscaling.targetCPUUtilizationPercentage` | Target CPU utilization percentage | `80` |

Specify each parameter using the `--set key=value[,key=value]` argument to `helm install`. For example:

```bash
helm install my-pm ./oprc-pm --set config.crm.default.url=http://my-crm:8088
```

Alternatively, a YAML file that specifies the values for the parameters can be provided while installing the chart:

```bash
helm install my-pm ./oprc-pm -f values.yaml
```

## Examples

### Basic Installation with Custom CRM URL

```bash
helm install pm ./oprc-pm \
  --set config.crm.default.url=http://my-crm.default.svc.cluster.local:8088
```

### NodePort Installation for External Access

```bash
helm install pm ./oprc-pm \
  --set service.type=NodePort \
  --set service.nodePort=30080 \
  --set config.crm.default.url=http://crm-service:8088
```

### Development Setup with Embedded Etcd

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=etcd \
  --set config.storage.etcd.useSubchart=true \
  --set etcd.enabled=true \
  --set etcd.replicaCount=1 \
  --set etcd.persistence.enabled=false
```

### Production Setup with Embedded Etcd HA

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=etcd \
  --set config.storage.etcd.useSubchart=true \
  --set etcd.enabled=true \
  --set etcd.replicaCount=3 \
  --set etcd.persistence.enabled=true \
  --set etcd.persistence.size=20Gi \
  --set etcd.auth.rbac.create=true \
  --set etcd.metrics.enabled=true
```

### Production Setup with External Etcd

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=etcd \
  --set config.storage.etcd.useSubchart=false \
  --set config.storage.etcd.endpoints=etcd-1.example.com:2379,etcd-2.example.com:2379 \
  --set config.storage.etcd.tls.insecure=false \
  --set resources.limits.cpu=500m \
  --set resources.limits.memory=512Mi \
  --set autoscaling.enabled=true
```

### Development Setup with Memory Storage

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=memory \
  --set config.deployment.rollbackOnPartial=true \
  --set config.package.deleteCascade=true \
  --set logging.level=debug
```

## Etcd Integration

This chart includes optional etcd integration through the Bitnami etcd subchart. You can:

1. **Use Embedded Etcd**: Set `etcd.enabled=true` and `config.storage.etcd.useSubchart=true`
2. **Use External Etcd**: Set `etcd.enabled=false` and configure `config.storage.etcd.endpoints`

### Embedded Etcd Features

- **High Availability**: Support for 3+ node clusters
- **Persistence**: Configurable persistent storage
- **Security**: RBAC and TLS authentication
- **Monitoring**: Prometheus metrics integration
- **Auto-compaction**: Configurable compaction policies

### External Etcd Features

- **TLS Support**: Full TLS client certificate authentication
- **Multiple Endpoints**: Load balancing across etcd cluster
- **Health Monitoring**: Circuit breaker and retry logic
  --set config.storage.etcd.tls.insecure=false \
  --set resources.limits.cpu=500m \
  --set resources.limits.memory=512Mi \
  --set autoscaling.enabled=true
```

### Development Setup with Memory Storage

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=memory \
  --set config.deployment.rollbackOnPartial=true \
  --set config.package.deleteCascade=true \
  --set logging.level=debug
```

### Multi-cluster Configuration

For multi-cluster deployments, you may want to configure multiple CRM endpoints through environment variables or ConfigMaps after installation.
