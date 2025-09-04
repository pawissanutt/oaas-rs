#!/usr/bin/env bash

# Knative minimal Serving deployment helper (no Eventing)
# Usage:
#   ./deploy-knative.sh install          # Install/ensure operator + serving
#   ./deploy-knative.sh uninstall        # Remove Knative Serving + operator (optional)
#   ./deploy-knative.sh status           # Show Knative components status
# Flags:
#   -n|--namespace <ns>   Namespace to install operator (default: knative-operator)
#   -d|--domain <domain>  Domain entry for config-domain (default: example.com)
#   --skip-repo           Skip helm repo add/update (assumes cached)
#   --force-recreate      Force delete existing Knative CRDs instead of attempting adoption
#   --helm-debug          Pass --debug to helm and show full output on failure
#   -h|--help             Show help

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log() { echo -e "${GREEN}[INFO]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()  { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

ACTION="install"
NAMESPACE="knative-operator"
DOMAIN="${KNATIVE_DOMAIN:-example.com}"
SKIP_REPO=false
FORCE_RECREATE=false
HELM_DEBUG=false

print_help() {
  cat <<EOF
Knative Serving deploy helper

Usage:
  ./deploy-knative.sh [install|uninstall|status|help] [flags]

Flags:
  -n, --namespace <ns>   Namespace to install operator (default: knative-operator)
  -d, --domain <domain>  Domain entry for config-domain (default: example.com)
      --skip-repo        Skip helm repo add/update (assumes cached)
      --force-recreate   Force delete existing Knative CRDs instead of adopting
      --helm-debug       Pass --debug to helm and show full output on failure
  -h, --help             Show this help

Env:
  KNATIVE_DOMAIN         Default domain (overridden by --domain)

Examples:
  ./deploy-knative.sh install -n oaas-dev
  ./deploy-knative.sh uninstall -n oaas-dev
  ./deploy-knative.sh install --force-recreate
EOF
}

# Parse args
if [[ $# -gt 0 ]]; then
  case "$1" in
    install|uninstall|status|help)
      ACTION="$1"; shift ;;
  esac
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--namespace) NAMESPACE="${2:-}"; shift 2;;
    -d|--domain) DOMAIN="${2:-}"; shift 2;;
    --skip-repo) SKIP_REPO=true; shift;;
    --force-recreate) FORCE_RECREATE=true; shift;;
    --helm-debug) HELM_DEBUG=true; shift;;
    -h|--help|help) ACTION="help"; shift;;
    *) warn "Unknown argument: $1"; shift;;
  esac
done

if [[ "$ACTION" == "help" ]]; then
  print_help
  exit 0
fi

command -v kubectl >/dev/null 2>&1 || err "kubectl is required"
command -v helm >/dev/null 2>&1 || err "helm is required"

# Ensure pre-existing Knative Operator CRDs (possibly created by earlier embedded chart) don't block Helm adoption
set_crd_ownership() {
  local crds=(
    "knativeservings.operator.knative.dev"
    "knativeeventings.operator.knative.dev"
  )
  for c in "${crds[@]}"; do
    if kubectl get crd "$c" >/dev/null 2>&1; then
      log "Patching existing CRD $c to set Helm ownership to knative-operator/$NAMESPACE"
      kubectl patch crd "$c" --type merge -p "{\"metadata\":{\"annotations\":{\"meta.helm.sh/release-name\":\"knative-operator\",\"meta.helm.sh/release-namespace\":\"$NAMESPACE\",\"meta.helm.sh/release-version\":\"0\",\"app.kubernetes.io/managed-by\":\"Helm\"}}}" >/dev/null 2>&1 || \
        warn "Failed to patch CRD $c annotations; Helm install may still fail"
    fi
  done
}

helm_install_operator() {
  local args=(upgrade --install knative-operator knative/knative-operator --namespace "$NAMESPACE" --create-namespace --wait)
  if [[ "$HELM_DEBUG" == true ]]; then args+=(--debug); fi
  if [[ "$HELM_DEBUG" == true ]]; then log "Helm command: helm ${args[*]}"; fi
  if ! out=$(helm "${args[@]}" 2>&1); then
    if [[ "$HELM_DEBUG" == true ]]; then
      echo -e "${RED}----- Helm Error Output Start -----${NC}" >&2
      echo "$out" >&2
      echo -e "${RED}----- Helm Error Output End -----${NC}" >&2
    fi
    return 1
  fi
}

delete_knative_crds_with_backup() {
  local crds=(
    "knativeservings.operator.knative.dev"
    "knativeeventings.operator.knative.dev"
  )
  for c in "${crds[@]}"; do
    if kubectl get crd "$c" >/dev/null 2>&1; then
      local backup="/tmp/${c}-backup.yaml"
      kubectl get crd "$c" -o yaml >"$backup" 2>/dev/null || true
      log "Deleting CRD $c (backup: $backup)"
      kubectl delete crd "$c" >/dev/null 2>&1 || true
    fi
  done
}

case "$ACTION" in
  status)
    log "Collecting Knative status..."
    kubectl get pods -n knative-serving || true
    kubectl get knativeserving -A || true
    ;;

  uninstall)
    log 'Removing Knative Serving CR...'
    kubectl delete knativeserving knative-serving -n knative-serving --ignore-not-found >/dev/null 2>&1 || true
    log 'Removing namespace knative-serving (will fail if other apps use it)'
    kubectl delete namespace knative-serving --ignore-not-found >/dev/null 2>&1 || true
    log 'Uninstalling Knative Operator Helm release (if in same namespace)'
    helm uninstall knative-operator -n "$NAMESPACE" >/dev/null 2>&1 || true
    ;;

  install)
    if [[ "$SKIP_REPO" == false ]]; then
      log 'Adding/updating Helm repo knative'
      helm repo add knative https://knative.github.io/operator >/dev/null 2>&1 || true
      helm repo update >/dev/null 2>&1 || true
    else
      log 'Skipping repo update'
    fi

    if [[ "$FORCE_RECREATE" == true ]]; then
      log 'ForceRecreate enabled: backing up and deleting existing Knative CRDs (if any)'
      delete_knative_crds_with_backup
    else
      log 'Reconciling existing Knative CRD ownership (if any)'
      set_crd_ownership
    fi

    log 'Installing / updating Knative Operator (attempt 1)'
    if ! helm_install_operator; then
      if [[ "$FORCE_RECREATE" == false ]]; then
        warn 'Initial Helm install failed. Attempting CRD purge + retry (non-destructive to data)'
        delete_knative_crds_with_backup
        log 'Retrying Helm install (attempt 2)'
        if ! helm_install_operator; then
          [[ "$HELM_DEBUG" == false ]] && warn 'Re-run with --helm-debug for detailed Helm diagnostics'
          err 'Failed to install knative-operator'
        fi
      else
        [[ "$HELM_DEBUG" == false ]] && warn 'Re-run with --helm-debug for detailed Helm diagnostics'
        err 'Failed to install knative-operator'
      fi
    fi

    log 'Ensuring knative-serving namespace'
    kubectl get ns knative-serving >/dev/null 2>&1 || kubectl create namespace knative-serving >/dev/null 2>&1

    log 'Applying KnativeServing CR'
    log "Using domain: ${DOMAIN}"
    cat <<YAML | kubectl apply -f - >/dev/null
apiVersion: operator.knative.dev/v1beta1
kind: KnativeServing
metadata:
  name: knative-serving
  namespace: knative-serving
spec:
  config:
    domain:
      "${DOMAIN}": ""
    network:
      ingress.class: kourier.ingress.networking.knative.dev
  ingress:
    kourier:
      enabled: true
YAML

    log 'Waiting for webhook service (up to 120s)'
    ok=false
    for i in $(seq 1 60); do
      if kubectl get svc webhook -n knative-serving >/dev/null 2>&1; then ok=true; break; fi
      sleep 2
    done
    if [[ "$ok" == true ]]; then log 'Webhook service ready'; else warn 'Webhook service not ready yet'; fi

    log 'Knative Serving install sequence complete'
    ;;

  *)
    print_help
    ;;
esac
