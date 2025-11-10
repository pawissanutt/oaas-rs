# OaaS Class Runtime Manager (CRM) Helm Chart

This Helm chart deploys the OaaS Class Runtime Manager (CRM), a Kubernetes controller that manages OaaS Class deployments through a custom resource definition (ClassRuntime).

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- ClassRuntime CRD installed (generated via crdgen). The deploy script handles this automatically.

## Installing the Chart

To install the chart with the release name `my-crm`:

```bash
helm install my-crm ./oprc-crm
```

## Uninstalling the Chart

To uninstall/delete the `my-crm` deployment:

```bash
helm delete my-crm
```

The command removes all the Kubernetes components associated with the chart and deletes the release.

## Configuration

The following table lists the configurable parameters of the CRM chart and their default values.

### Global Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of CRM replicas | `1` |
| `image.repository` | CRM image repository | `ghcr.io/pawissanutt/oaas-rs/crm` |
| `image.tag` | CRM image tag | `latest` |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `nameOverride` | String to partially override the fullname | `""` |
| `fullnameOverride` | String to fully override the fullname | `""` |

### Service Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Kubernetes Service type (ClusterIP/NodePort/LoadBalancer) | `ClusterIP` |
| `service.port` | Service port | `8088` |
| `service.targetPort` | Container target port | `8088` |
| `service.nodePort` | NodePort (30000-32767, only when type=NodePort) | `30088` |

### CRM Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.httpPort` | HTTP port for gRPC+HTTP server | `8088` |
| `config.namespace` | Kubernetes namespace for CRD operations | `default` |
| `config.features.odgm` | Enable ODGM addon support | `true` |
| `config.features.knative` | (Deprecated) Knative template support (always disabled in chart) | `false` |
| `config.features.prometheus` | Enable Prometheus metrics provider | `false` |
| `config.features.nfrEnforcement` | Enable NFR enforcement | `false` |
| `config.features.hpa` | Enable HPA support | `false` |
| `config.features.fsm` | Enable FSM-based reconcile (template descriptors + readiness rules) | `false` |
| `config.profile` | Environment profile (dev, edge, full) | `dev` |
| `config.templates.odgmImageOverride` | Override ODGM sidecar image used by templates (sets env OPRC_CRM_TEMPLATES_ODGM_IMAGE) | `""` |


### RBAC & CRD

| Parameter | Description | Default |
|-----------|-------------|---------|
| `rbac.create` | Create RBAC resources | `true` |
| `crd.create` | Install CRD | `false` (CRDs are generated and applied externally) |
| `crd.keep` | Keep CRDs on chart deletion | `true` |

### Autoscaling

| Parameter | Description | Default |
|-----------|-------------|---------|
| `autoscaling.enabled` | Enable horizontal pod autoscaler | `false` |
| `autoscaling.minReplicas` | Minimum number of replicas | `1` |
| `autoscaling.maxReplicas` | Maximum number of replicas | `3` |
| `autoscaling.targetCPUUtilizationPercentage` | Target CPU utilization percentage | `80` |

Specify each parameter using the `--set key=value[,key=value]` argument to `helm install`. For example:

```bash
helm install my-crm ./oprc-crm --set config.features.nfrEnforcement=true
```

Alternatively, a YAML file that specifies the values for the parameters can be provided while installing the chart:

```bash
helm install my-crm ./oprc-crm -f values.yaml
```

## CRDs are managed externally

- The CRD is generated from source with `crdgen` (cargo run -p oprc-crm --bin crdgen).
- The deploy script applies the generated CRD before installing any CRM release.
- This keeps the cluster’s CRDs in sync with the code and avoids template drift.

## Examples

### Basic Installation with NFR Enforcement

```bash
helm install crm ./oprc-crm \
  --set config.features.nfrEnforcement=true \
  --set config.features.prometheus=true \
  --set config.profile=full
```

### NodePort Installation for External Access

```bash
helm install crm ./oprc-crm \
  --set service.type=NodePort \
  --set service.nodePort=30088 \
  --set config.profile=dev
```

### Production Setup with Custom Image

```bash
helm install crm ./oprc-crm \
  --set image.tag=v1.0.0 \
  --set resources.limits.cpu=500m \
  --set resources.limits.memory=512Mi \
  --set resources.requests.cpu=100m \
  --set resources.requests.memory=128Mi \
  --set autoscaling.enabled=true
```

### Override ODGM sidecar image

To force CRM to use a specific ODGM sidecar image in all rendered templates:

```bash
helm install crm ./oprc-crm \
  --set config.templates.odgmImageOverride="ghcr.io/your-org/odgm:custom"
```

### Enable Prometheus Operator Integration

For automated monitoring setup:

```bash
# Install CRM with Prometheus Operator
helm install crm ./oprc-crm \
  --set prometheus.operator.enabled=true \
  --set prometheus.serviceMonitor.enabled=true \
  --set prometheus.prometheusRule.enabled=true
```
