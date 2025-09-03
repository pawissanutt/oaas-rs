#!/usr/bin/env bash

# OaaS deploy script (single profile)
# - Installs PM (with embedded or external etcd)
# - Installs N CRM releases into separate namespaces (numeric index)

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
log(){ echo -e "${GREEN}[INFO]${NC} $*"; }
warn(){ echo -e "${YELLOW}[WARN]${NC} $*"; }
error(){ echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

ACTION="${1:-deploy}"; shift 1 2>/dev/null || true
CHARTS_DIR="$(cd "$(dirname "$0")" && pwd)"

# Defaults
PM_NS="oaas"
PM_RELEASE="oaas-pm"

# CRM defaults (numeric series)
CRM_COUNT=${CRM_COUNT:-2}
CRM_NS_PREFIX="${CRM_NS_PREFIX:-oaas}"
CRM_RELEASE_PREFIX="${CRM_RELEASE_PREFIX:-oaas-crm}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pm-ns) PM_NS="${2:-}"; shift 2;;
  --crm-count) CRM_COUNT="${2:-2}"; shift 2;;
  --crm-ns-prefix) CRM_NS_PREFIX="${2:-oaas}"; shift 2;;
  --crm-release-prefix) CRM_RELEASE_PREFIX="${2:-oaas-crm}"; shift 2;;
  --purge-namespaces) PURGE_NAMESPACES=true; shift;;
  --no-clean) CLEAN_RESOURCES=false; shift;;
    --help|-h|help) ACTION="help"; shift;;
    *) warn "Unknown flag: $1"; shift;;
  esac
done

show_help(){ cat <<EOF
OaaS deploy (single profile)

Installs:
  - PM in namespace: 
      ${PM_NS}
  - CRM instances (1..N) using namespaces:
      ${CRM_NS_PREFIX}-1 .. ${CRM_NS_PREFIX}-${CRM_COUNT}

Usage:
  ./deploy.sh [deploy|undeploy|status|help]
Optional flags:
  --pm-ns <ns>                Namespace for PM (default: oaas)
  --crm-count <n>             Number of CRMs to deploy (default: 2)
  --crm-ns-prefix <prefix>    Namespace prefix for CRMs (default: oaas)
  --crm-release-prefix <pfx>  Release prefix for CRMs (default: oaas-crm)
  --purge-namespaces          Also delete namespaces during undeploy (destructive)
  --no-clean                  Skip extra cleanup of leftover labeled resources on undeploy
EOF
}

ensure_tools(){
  command -v helm >/dev/null 2>&1 || error "helm is not installed"
  command -v kubectl >/dev/null 2>&1 || error "kubectl is not installed"
  kubectl cluster-info >/dev/null 2>&1 || error "Cannot connect to Kubernetes cluster"
}

ensure_ns(){
  local ns="$1"; kubectl get ns "$ns" >/dev/null 2>&1 || kubectl create namespace "$ns" >/dev/null
}

# Computed names
crm_release_name(){ local i="$1"; echo "${CRM_RELEASE_PREFIX}-${i}"; }
crm_namespace(){ local i="$1"; echo "${CRM_NS_PREFIX}-${i}"; }

# Values files
ensure_crm_values_file(){
  local i="$1"; local path="$CHARTS_DIR/examples/crm-${i}.yaml"
  if [[ ! -f "$path" ]]; then
    log "Creating missing CRM values file: examples/crm-${i}.yaml"
    cat >"$path" <<YAML
# CRM ${i} values (override as needed)
{}
YAML
  fi
  echo "$path"
}
ensure_pm_values_file(){
  local path="$CHARTS_DIR/examples/pm.yaml"
  if [[ ! -f "$path" ]]; then
    log "Creating missing PM values file: examples/pm.yaml"
    echo '{}' >"$path"
  fi
  echo "$path"
}

# Generate and apply CRM CRDs from source (keeps cluster CRDs up-to-date)
generate_and_apply_crd(){
  log "Generating CRM CRD via crdgen and applying to cluster"
  # Run from repo workspace (any subdir is fine; cargo resolves workspace root)
  # Store a copy under k8s/crds for reference
  cargo run -p oprc-crm --bin crdgen | tee "$CHARTS_DIR/../crds/classruntimes.gen.yaml" | kubectl apply -f -
}

install_or_upgrade_crm(){
  local rel="$1" ns="$2" cmd="$3" values_file="$4"
  ensure_ns "$ns"
  log "${cmd^} CRM release $rel in $ns"
  helm upgrade --install "$rel" "$CHARTS_DIR/oprc-crm" \
    --namespace "$ns" --create-namespace \
    --values "$values_file" \
  --set crd.create=false \
    --set config.namespace="$ns" \
    --wait
}

install_or_upgrade_pm(){
  local cmd="$1"
  ensure_ns "$PM_NS"
  local crm1_rel="$(crm_release_name 1)"
  local crm1_ns="$(crm_namespace 1)"
  local crm1_svc_fqdn="http://${crm1_rel}-oprc-crm.${crm1_ns}.svc.cluster.local:8088"
  log "${cmd^} PM release $PM_RELEASE in $PM_NS (CRM default: $crm1_svc_fqdn)"
  local pm_values
  pm_values="$(ensure_pm_values_file)"
  helm upgrade --install "$PM_RELEASE" "$CHARTS_DIR/oprc-pm" \
    --namespace "$PM_NS" --create-namespace \
    --values "$pm_values" \
    --set config.crm.default.url="$crm1_svc_fqdn" \
    --wait
}

uninstall_release(){
  local rel="$1" ns="$2"; log "Uninstalling $rel from $ns"; helm uninstall "$rel" -n "$ns" >/dev/null 2>&1 || true
}

clean_labeled_resources(){
  local rel="$1" ns="$2"
  log "Cleaning leftover resources labeled app.kubernetes.io/instance=${rel} in namespace ${ns}"
  # Namespaced resources
  kubectl api-resources --verbs=list --namespaced -o name 2>/dev/null | \
    xargs -r -I{} kubectl delete {} -n "$ns" -l app.kubernetes.io/instance="$rel" --ignore-not-found >/dev/null 2>&1 || true
  # Cluster-scoped resources created by the release (rare but possible)
  kubectl api-resources --verbs=list --namespaced=false -o name 2>/dev/null | \
    xargs -r -I{} kubectl delete {} -l app.kubernetes.io/instance="$rel" --ignore-not-found >/dev/null 2>&1 || true
}

delete_namespace_if_requested(){
  local ns="$1"
  if [[ "${PURGE_NAMESPACES:-false}" == true ]]; then
    log "Deleting namespace: $ns"
    kubectl delete namespace "$ns" --ignore-not-found >/dev/null 2>&1 || true
  fi
}

:
CLEAN_RESOURCES=${CLEAN_RESOURCES:-true}
PURGE_NAMESPACES=${PURGE_NAMESPACES:-false}

status_ns(){
  local ns="$1"; echo "=== Namespace: $ns ==="; helm list -n "$ns" || true; kubectl get all -n "$ns" || true; echo
}

case "$ACTION" in
  help|-h|--help)
  show_help ; exit 0 ;;

  deploy|install|upgrade)
    ensure_tools
  # Ensure CRDs are present/up-to-date before installing any CRM releases
  generate_and_apply_crd
    for i in $(seq 1 "$CRM_COUNT"); do
      rel="$(crm_release_name "$i")"; ns="$(crm_namespace "$i")"
      values_file="$(ensure_crm_values_file "$i")"
      install_or_upgrade_crm "$rel" "$ns" install "$values_file"
    done
    install_or_upgrade_pm install
    log "Deploy completed. PM namespace: $PM_NS; CRMs: ${CRM_NS_PREFIX}-1..${CRM_NS_PREFIX}-${CRM_COUNT}" ;;

  undeploy|uninstall)
    ensure_tools
    uninstall_release "$PM_RELEASE" "$PM_NS"
    for i in $(seq "$CRM_COUNT" -1 1); do
      rel="$(crm_release_name "$i")"; ns="$(crm_namespace "$i")"
      uninstall_release "$rel" "$ns"
    done
    if [[ "$CLEAN_RESOURCES" == true ]]; then
      clean_labeled_resources "$PM_RELEASE" "$PM_NS"
      for i in $(seq "$CRM_COUNT" -1 1); do
        rel="$(crm_release_name "$i")"; ns="$(crm_namespace "$i")"
        clean_labeled_resources "$rel" "$ns"
      done
    else
      warn "Skipping cleanup of leftover resources (--no-clean)"
    fi
    delete_namespace_if_requested "$PM_NS"
    for i in $(seq "$CRM_COUNT" -1 1); do
      ns="$(crm_namespace "$i")"; delete_namespace_if_requested "$ns"
    done
    log "Undeploy completed." ;;

  status)
    ensure_tools
  status_ns "$PM_NS"
  for i in $(seq 1 "$CRM_COUNT"); do ns="$(crm_namespace "$i")"; status_ns "$ns"; done ;;

  *)
    show_help; error "Unknown action: $ACTION" ;;
esac

log "Action '$ACTION' completed."

