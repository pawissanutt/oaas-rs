#!/bin/bash
set -e

echo "Deploying Observability Stack (VictoriaMetrics & Grafana)..."

# 1. Add Helm Repos
helm repo add vm https://victoriametrics.github.io/helm-charts/
helm repo add grafana https://grafana.github.io/helm-charts
helm repo update

# 2. Create Namespace
kubectl create ns observability --dry-run=client -o yaml | kubectl apply -f -

# 3. Deploy VictoriaMetrics Single (Metrics storage + OTLP Receiver)
# We enable OTLP so Rust apps can push metrics directly to port 4317
helm upgrade --install vm-single vm/victoria-metrics-single \
  --namespace observability \
  --set server.extraArgs.opentelemetry=1

# 4. Deploy VictoriaLogs (Log storage + OTLP Receiver)
# We enable OTLP so Rust apps can push logs directly
helm upgrade --install vm-logs vm/victoria-logs-single \
  --namespace observability \
  --set server.extraArgs.opentelemetry=1

# 5. Deploy Grafana
helm upgrade --install grafana grafana/grafana \
  --namespace observability \
  --set adminPassword=admin \
  --set service.type=NodePort

echo "Observability stack deployed."
echo "VM Metrics OTLP: http://vm-single-server.observability.svc.cluster.local:4317"
echo "VM Logs OTLP:    http://vm-logs-single-server.observability.svc.cluster.local:4317"
