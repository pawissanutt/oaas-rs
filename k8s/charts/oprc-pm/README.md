# OaaS Package Manager (PM) Helm Chart

Deploys the OaaS Package Manager (PM) to manage packages and orchestrate class/function deployments via the CRM.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- CRM (Class Runtime Manager) reachable from the PM

## Install

Install with the release name `my-pm` from the `k8s/charts` folder:

```bash
helm install my-pm ./oprc-pm
```

## Uninstall

```bash
helm delete my-pm
```

## Configuration

Defaults live in `values.yaml`. Common knobs:

### Global

- `replicaCount`: `1`
- `image.repository`: `ghcr.io/pawissanutt/oaas-rs/pm`
- `image.tag`: `latest`
- `image.pullPolicy`: `IfNotPresent`
- `nameOverride`, `fullnameOverride`: `""`

### Service

- `service.type`: `ClusterIP` \| `NodePort` \| `LoadBalancer` (default `ClusterIP`)
- `service.port`: `8080`, `service.targetPort`: `8080`
- `service.nodePort`: `30080` (only when type=NodePort)

### Server

- `config.server.host`: `0.0.0.0`
- `config.server.port`: `8080`
- `config.server.workers`: `null`

### Storage backends

Two supported modes:

1. Memory (default)
   - `config.storage.type`: `memory`

2. External etcd
   - `config.storage.type`: `etcd`
   - `config.storage.etcd.endpoints`: comma-separated host:port list (e.g. `etcd-1:2379,etcd-2:2379`)
   - TLS (optional):
     - `config.storage.etcd.tls.insecure`: `true`|`false`
     - `config.storage.etcd.tls.caCertPath`, `clientCertPath`, `clientKeyPath`

Deprecated: embedded etcd subchart

- Older versions allowed enabling a bundled etcd via subchart. This path is deprecated and will be removed.
- Prefer external etcd or memory.
- If you must use it temporarily: set `etcd.enabled=true`, `config.storage.type=etcd`, and `config.storage.etcd.useSubchart=true`. Note this is not the recommended or future-proof setup.

### CRM

- `config.crm.default.url`: `http://oprc-crm:8088`
- `config.crm.default.timeout`: `30`
- `config.crm.default.retryAttempts`: `3`
- `config.crm.healthCheckInterval`: `60`
- `config.crm.healthCacheTtl`: `15`

### Deployment policy

- `config.deployment.maxRetries`: `2`
- `config.deployment.rollbackOnPartial`: `false`

### Package management

- `config.package.deleteCascade`: `false`

### Autoscaling

- `autoscaling.enabled`: `false`
- `autoscaling.minReplicas`: `1`
- `autoscaling.maxReplicas`: `3`
- `autoscaling.targetCPUUtilizationPercentage`: `80`

### Observability and logging

- `logging.level`: `info` (use `debug` for development)
- `logging.format`: `plain`
- `config.observability.enabled`: `true`
- `config.observability.tracing.enabled`: `false`
- `config.observability.tracing.endpoint`: `""`

You can override any value with `--set key=value` or provide a file with `-f my-values.yaml`.

## Examples

### Basic with custom CRM URL

```bash
helm install pm ./oprc-pm \
  --set config.crm.default.url=http://my-crm.default.svc.cluster.local:8088
```

### NodePort for external access

```bash
helm install pm ./oprc-pm \
  --set service.type=NodePort \
  --set service.nodePort=30080 \
  --set config.crm.default.url=http://crm-service:8088
```

### Development with memory storage

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=memory \
  --set config.deployment.rollbackOnPartial=true \
  --set config.package.deleteCascade=true \
  --set logging.level=debug
```

### Production with external etcd

```bash
helm install pm ./oprc-pm \
  --set config.storage.type=etcd \
  --set config.storage.etcd.endpoints=etcd-1.example.com:2379,etcd-2.example.com:2379 \
  --set config.storage.etcd.tls.insecure=false \
  --set resources.limits.cpu=500m \
  --set resources.limits.memory=512Mi \
  --set autoscaling.enabled=true
```

## Notes

- The embedded etcd subchart is deprecated and will be removed in a future release. Plan migrations to external etcd or use memory for development.
- For multi-cluster setups, point PM at a CRM front-door or handle multiple CRM endpoints via ConfigMap/env after install.
