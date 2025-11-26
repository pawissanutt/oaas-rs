#!/bin/bash
set -e


function install() {
    echo "Deploying Observability Stack (VictoriaMetrics & Grafana)..."

    # 1. Add Helm Repositories
    helm repo add vm https://victoriametrics.github.io/helm-charts/
    helm repo add grafana https://grafana.github.io/helm-charts
    helm repo add open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
    helm repo update

    # 2. Create Namespace
    kubectl create namespace observability --dry-run=client -o yaml | kubectl apply -f -

    # 3. Deploy Victoria Metrics (Single Node)
    helm upgrade --install vm-single vm/victoria-metrics-single \
      --namespace observability

    # 4. Deploy Victoria Logs (Single Node)
    helm upgrade --install vm-logs vm/victoria-logs-single \
      --namespace observability

    # 5. Deploy Victoria Traces (Single Node)
    helm upgrade --install vm-traces vm/victoria-traces-single \
      --namespace observability

    # 6. Deploy OpenTelemetry Collector
    helm upgrade --install otel-collector open-telemetry/opentelemetry-collector \
      --namespace observability \
      -f "$(dirname "${BASH_SOURCE[0]}")/otel-collector-values.yaml"

    # 7. Deploy Grafana
    helm upgrade --install grafana grafana/grafana \
      --namespace observability \
      --set adminPassword=admin \
      --set service.type=NodePort \
      -f "$(dirname "${BASH_SOURCE[0]}")/grafana-values.yaml"

    echo "Observability stack deployed."
    echo "OTEL Collector: http://otel-collector-opentelemetry-collector.observability.svc.cluster.local:4317"
    echo "Grafana:        http://grafana.observability.svc.cluster.local"
}

function uninstall() {
    echo "Uninstalling Observability Stack..."
    helm uninstall vm-single -n observability || true
    helm uninstall vm-logs -n observability || true
    helm uninstall vm-traces -n observability || true
    helm uninstall otel-collector -n observability || true
    helm uninstall grafana -n observability || true
    kubectl delete ns observability --ignore-not-found
    echo "Waiting for namespace deletion..."
    kubectl wait --for=delete namespace/observability --timeout=60s || true
    echo "Observability stack uninstalled."
}

if [ "$1" == "install" ]; then
    install
elif [ "$1" == "uninstall" ]; then
    uninstall
else
    echo "Usage: $0 [install|uninstall]"
    exit 1
fi
