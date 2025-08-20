# OaaS Helm Charts

This directory contains Helm charts for deploying OaaS (Object-as-a-Service) components on Kubernetes.

## Charts

### oprc-crm (Class Runtime Manager)
The Class Runtime Manager is a Kubernetes controller that manages OaaS Class deployments through custom resources (DeploymentRecord CRD).

**Key Features:**
- Custom Resource Definition (CRD) for DeploymentRecord
- RBAC configuration for Kubernetes API access
- Support for ODGM (Object Data Grid Manager) addon
- NFR (Non-Functional Requirements) enforcement
- Template-based deployment strategies (dev/edge/full/knative)

**Location:** `./oprc-crm/`

### oprc-pm (Package Manager)
The Package Manager orchestrates OaaS package and class deployments across one or more CRM-managed clusters via REST API and gRPC communication.

**Key Features:**
- REST API for package and deployment management
- Multi-cluster deployment orchestration
- Pluggable storage backends (memory/etcd with embedded or external options)
- Embedded etcd subchart with HA support and persistence
- Circuit breaker and retry mechanisms
- Health monitoring and caching
- TLS support for external etcd clusters

**Location:** `./oprc-pm/`

## Quick Start

### Prerequisites
- Kubernetes cluster (1.19+)
- Helm 3.0+
- kubectl configured for your cluster

### 1. Install CRM (Class Runtime Manager)

```bash
# Install CRM with default configuration
helm install crm ./oprc-crm

# Or with custom values
helm install crm ./oprc-crm -f examples/crm-production.yaml
```

### 2. Install PM (Package Manager)

```bash
# Install PM with CRM dependency (memory storage)
helm install pm ./oprc-pm --set config.crm.default.url=http://crm-oprc-crm:8088

# Install PM with embedded etcd (development)
helm install pm ./oprc-pm \
  --set config.crm.default.url=http://crm-oprc-crm:8088 \
  --set config.storage.type=etcd \
  --set config.storage.etcd.useSubchart=true \
  --set etcd.enabled=true

# Or with custom values
helm install pm ./oprc-pm -f examples/pm-production.yaml
```

### 2a. Alternative: NodePort Installation for External Access

For development/testing environments where you need external access without ingress:

```bash
# Install CRM with NodePort
helm install crm ./oprc-crm \
  --set service.type=NodePort \
  --set service.nodePort=30088

# Install PM with NodePort
helm install pm ./oprc-pm \
  --set service.type=NodePort \
  --set service.nodePort=30080 \
  --set config.crm.default.url=http://crm-oprc-crm:8088

# Access services externally
# CRM: http://<node-ip>:30088/health
# PM:  http://<node-ip>:30080/health
```

### 3. Verify Installation

```bash
# Check CRM deployment
kubectl get deploymentrecords.oaas.io
kubectl get pods -l app.kubernetes.io/name=oprc-crm

# Check PM deployment
kubectl get pods -l app.kubernetes.io/name=oprc-pm

# Test PM API
kubectl port-forward svc/pm-oprc-pm 8080:8080
curl http://localhost:8080/health
```

## Configuration Examples

### Development Environment

For development environments, use memory storage and enable debugging:

```yaml
# dev-values.yaml
crm:
  config:
    profile: dev
    features:
      odgm: true
      nfrEnforcement: false
  logging:
    level: debug

pm:
  config:
    storage:
      type: memory
    deployment:
      rollbackOnPartial: true
    package:
      deleteCascade: true
  logging:
    level: debug
```

### Production Environment

For production environments, use etcd storage, enable monitoring, and configure resource limits:

```yaml
# prod-values.yaml
crm:
  config:
    profile: full
    features:
      odgm: true
      nfrEnforcement: true
      prometheus: true
  resources:
    limits:
      cpu: 500m
      memory: 512Mi
    requests:
      cpu: 100m
      memory: 128Mi
  autoscaling:
    enabled: true

pm:
  config:
    storage:
      type: etcd
      etcd:
        endpoints: "etcd-cluster:2379"
        tls:
          insecure: false
  resources:
    limits:
      cpu: 500m
      memory: 512Mi
    requests:
      cpu: 100m
      memory: 128Mi
  autoscaling:
    enabled: true
```

### NodePort Environment

For development/testing with external access without ingress controller:

```yaml
# nodeport-values.yaml
crm:
  service:
    type: NodePort
    port: 8088
    nodePort: 30088
  config:
    profile: dev
    features:
      odgm: true

pm:
  service:
    type: NodePort
    port: 8080
    nodePort: 30080
  config:
    storage:
      type: etcd
  etcd:
    enabled: true
    persistence:
      enabled: false
```

Access services via:
- CRM: `http://<node-ip>:30088`
- PM: `http://<node-ip>:30080`

## Deployment Strategies

### Single Cluster Deployment

Deploy both CRM and PM in the same cluster:

```bash
# Install CRM first
helm install crm ./oprc-crm

# Then install PM pointing to CRM
helm install pm ./oprc-pm \
  --set config.crm.default.url=http://crm-oprc-crm:8088
```

### Multi-Cluster Deployment

Deploy PM in a management cluster and CRM in target clusters:

```bash
# In management cluster - install PM only
helm install pm ./oprc-pm \
  --set config.crm.default.url=https://target-cluster-crm.example.com:8088

# In each target cluster - install CRM only
helm install crm ./oprc-crm \
  --set ingress.enabled=true \
  --set ingress.hosts[0].host=target-cluster-crm.example.com
```

### NodePort Deployment

For development and testing environments where external access is needed without ingress:

```bash
# Deploy with NodePort access for both components
./deploy.sh nodeport install

# Or manually install with NodePort configuration
helm install crm ./oprc-crm \
  --set service.type=NodePort \
  --set service.nodePort=30088

helm install pm ./oprc-pm \
  --set service.type=NodePort \
  --set service.nodePort=30080
```

Access the services externally:
- CRM: `http://<node-ip>:30088`
- PM: `http://<node-ip>:30080`

Get node IP:
```bash
kubectl get nodes -o wide
```

### Operator-Enhanced Deployment

For enhanced functionality with automatic operator installation:

```bash
# Deploy with both Prometheus and Knative operators
./deploy.sh dev install --operators

# Deploy with Prometheus operator only (monitoring)
./deploy.sh prod install --prometheus

# Deploy with Knative operator only (serverless)
./deploy.sh dev install --knative

# PowerShell equivalents
.\deploy.ps1 dev install -Operators
.\deploy.ps1 prod install -Prometheus
.\deploy.ps1 dev install -Knative
```

**Operator Features:**
- **Prometheus**: Automatic ServiceMonitor and PrometheusRule creation
- **Knative**: Serverless runtime with Kourier ingress and Magic DNS
- **Combined**: Full observability + serverless capabilities

## Monitoring and Observability

### Prometheus Integration

Enable Prometheus monitoring for CRM:

```bash
helm install crm ./oprc-crm \
  --set config.features.prometheus=true \
  --set config.features.nfrEnforcement=true
```

### Logging Configuration

Configure structured logging:

```bash
# JSON logs for production
helm install crm ./oprc-crm --set logging.format=json
helm install pm ./oprc-pm --set logging.format=json

# Debug logs for development
helm install crm ./oprc-crm --set logging.level=debug
helm install pm ./oprc-pm --set logging.level=debug
```

## Operator Integration

The CRM chart supports optional operator integrations for enhanced functionality:

### Prometheus Operator

Enable automated monitoring with Prometheus Operator:

```bash
# Install CRM with Prometheus Operator support
helm install crm ./oprc-crm \
  --set prometheus.operator.enabled=true \
  --set prometheus.serviceMonitor.enabled=true \
  --set prometheus.prometheusRule.enabled=true
```

This automatically creates:
- ServiceMonitor for metrics collection
- PrometheusRule for alerting rules
- Installs Prometheus Operator if not present

### Knative Operator

Enable serverless runtime support with Knative Serving:

```bash
# Install CRM with Knative Operator (Serving only)
helm install crm ./oprc-crm \
  --set knative.operator.enabled=true \
  --set knative.operator.serving.enabled=true \
  --set knative.operator.serving.ingress.kourier.enabled=true
```

**Note**: Knative Eventing has been removed as it's unnecessary for most OaaS use cases. The chart now focuses on Knative Serving for serverless function execution.

Features included:
- Automatic Knative Operator installation
- Knative Serving component setup
- Kourier ingress controller (lightweight)
- Magic DNS for development environments
- Proper RBAC permissions

### Combined Operators Example

```bash
# Full operator integration
helm install crm ./oprc-crm -f examples/crm-with-operators.yaml
```

## Security Considerations

### Service Accounts

Both charts create dedicated service accounts with minimal required permissions:

- CRM: Requires access to Kubernetes APIs for managing deployments, services, and custom resources
- PM: Minimal permissions, primarily for health checks

### Network Policies

Consider implementing network policies to restrict communication:

```yaml
# Allow PM to communicate with CRM
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: pm-to-crm
spec:
  podSelector:
    matchLabels:
      app.kubernetes.io/name: oprc-pm
  policyTypes:
  - Egress
  egress:
  - to:
    - podSelector:
        matchLabels:
          app.kubernetes.io/name: oprc-crm
    ports:
    - protocol: TCP
      port: 8088
```

### TLS Configuration

For production deployments, enable TLS:

```bash
# Enable ingress with TLS
helm install crm ./oprc-crm \
  --set ingress.enabled=true \
  --set ingress.tls[0].secretName=crm-tls \
  --set ingress.tls[0].hosts[0]=crm.example.com

helm install pm ./oprc-pm \
  --set ingress.enabled=true \
  --set ingress.tls[0].secretName=pm-tls \
  --set ingress.tls[0].hosts[0]=pm.example.com
```

## Troubleshooting

### Common Issues

1. **CRD Installation Failures**
   ```bash
   # Check if CRDs are properly installed
   kubectl get crd deploymentrecords.oaas.io
   
   # Manually install CRDs if needed
   kubectl apply -f ./oprc-crm/templates/crd.yaml
   ```

2. **PM Cannot Connect to CRM**
   ```bash
   # Check CRM service
   kubectl get svc -l app.kubernetes.io/name=oprc-crm
   
   # Test connectivity
   kubectl exec -it deploy/pm-oprc-pm -- curl http://crm-oprc-crm:8088/health
   ```

3. **RBAC Permission Issues**
   ```bash
   # Check CRM permissions
   kubectl auth can-i create deploymentrecords.oaas.io --as=system:serviceaccount:default:crm-oprc-crm
   ```

### Debug Mode

Enable debug logging:

```bash
helm upgrade crm ./oprc-crm --set logging.level=debug
helm upgrade pm ./oprc-pm --set logging.level=debug
```

### Health Checks

Verify component health:

```bash
# CRM health
kubectl port-forward svc/crm-oprc-crm 8088:8088
curl http://localhost:8088/health

# PM health
kubectl port-forward svc/pm-oprc-pm 8080:8080
curl http://localhost:8080/health
```

## Available Example Files

The `examples/` directory contains pre-configured values files for common deployment scenarios:

### Individual Component Examples
- `crm-development.yaml` - CRM with development settings (memory storage, debug logging)
- `crm-production.yaml` - CRM with production settings (monitoring enabled, resource limits)
- `crm-nodeport.yaml` - CRM with NodePort service for external access
- `crm-with-operators.yaml` - CRM with Prometheus and Knative operators enabled
- `crm-knative-dev.yaml` - CRM with Knative Serving for development (eventing removed)
- `pm-development.yaml` - PM with development settings (memory storage, debug logging)
- `pm-production.yaml` - PM with production settings (etcd storage, resource limits)
- `pm-nodeport.yaml` - PM with NodePort service and embedded etcd

### Combined Deployment Examples
- `combined-deployment.yaml` - Both components with staging configuration
- `combined-nodeport.yaml` - Both components with NodePort access for development

Usage:
```bash
# Use specific example
helm install crm ./oprc-crm -f examples/crm-nodeport.yaml

# Deploy complete environment
./deploy.sh nodeport install

# Deploy with operators
./deploy.sh dev install --operators
./deploy.sh prod install --prometheus
```

## Contributing

When modifying charts:

1. Update version in `Chart.yaml`
2. Test with different Kubernetes versions
3. Validate with `helm lint`
4. Update documentation
