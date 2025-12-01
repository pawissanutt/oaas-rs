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
PM_DOMAIN="${PM_DOMAIN:-}"
REGISTRY_PREFIX="${REGISTRY_PREFIX:-}"
IMAGE_TAG="${IMAGE_TAG:-latest}"

# CRM defaults (numeric series)
CRM_COUNT=${CRM_COUNT:-2}
CRM_NS_PREFIX="${CRM_NS_PREFIX:-oaas}"
CRM_RELEASE_PREFIX="${CRM_RELEASE_PREFIX:-oaas-crm}"

# Bundle defaults
BUNDLE_OUTPUT="${BUNDLE_OUTPUT:-deploy-all.yaml}"
BUNDLE_SKIP_CRD=${BUNDLE_SKIP_CRD:-false}
BUNDLE_SINGLE=${BUNDLE_SINGLE:-false}

# Image registry override
IMAGE_REGISTRY="${IMAGE_REGISTRY:-}"
VALUES_PREFIX="${VALUES_PREFIX:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pm-ns) PM_NS="${2:-}"; shift 2;;
  --pm-domain) PM_DOMAIN="${2:-}"; shift 2;;
  --registry) REGISTRY_PREFIX="${2:-}"; shift 2;;
  --tag) IMAGE_TAG="${2:-latest}"; shift 2;;
  --crm-count) CRM_COUNT="${2:-2}"; shift 2;;
  --crm-ns-prefix) CRM_NS_PREFIX="${2:-oaas}"; shift 2;;
  --crm-release-prefix) CRM_RELEASE_PREFIX="${2:-oaas-crm}"; shift 2;;
  --registry) IMAGE_REGISTRY="${2:-}"; shift 2;;
  --values-prefix) VALUES_PREFIX="${2:-}"; shift 2;;
  --bundle-output) BUNDLE_OUTPUT="${2:-deploy-all.yaml}"; shift 2;;
  --bundle-skip-crd) BUNDLE_SKIP_CRD=true; shift;;
  --bundle-single) BUNDLE_SINGLE=true; shift;;
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
  --pm-domain <domain>        Enable PM Ingress and use this domain as host (e.g. pm.example.com)
  --registry <prefix>         Image registry prefix (e.g. localhost:5000 or myuser). Overrides default ghcr.io/...
  --tag <tag>                 Image tag to use (default: latest)
  --crm-count <n>             Number of CRMs to deploy (default: 2)
  --crm-ns-prefix <prefix>    Namespace prefix for CRMs (default: oaas)
  --crm-release-prefix <pfx>  Release prefix for CRMs (default: oaas-crm)
  --bundle-output <file>      Output path for 'bundle' action (default: deploy-all.yaml)
  --bundle-skip-crd           When using 'bundle', omit generating/appending CRD
  --bundle-single             Bundle using single-namespace mode with examples/{crm-single,pm-single}.yaml
  --purge-namespaces          Also delete namespaces during undeploy (destructive)
  --no-clean                  Skip extra cleanup of leftover labeled resources on undeploy
EOF
}

ensure_tools(){
  command -v helm >/dev/null 2>&1 || error "helm is not installed"
  command -v kubectl >/dev/null 2>&1 || error "kubectl is not installed"
  if [[ "$ACTION" != "bundle" ]]; then
    kubectl cluster-info >/dev/null 2>&1 || error "Cannot connect to Kubernetes cluster"
  else
    # Bundle action can be run offline (no cluster connectivity required)
    if ! kubectl version --client >/dev/null 2>&1; then
      error "kubectl client not functioning"
    fi
  fi
}

ensure_ns(){
  local ns="$1"; kubectl get ns "$ns" >/dev/null 2>&1 || kubectl create namespace "$ns" >/dev/null
}

get_crm_image_args(){
  if [[ -n "$REGISTRY_PREFIX" ]]; then
    echo "--set image.repository=${REGISTRY_PREFIX}/crm \
          --set image.tag=${IMAGE_TAG} \
          --set router.image.repository=${REGISTRY_PREFIX}/router \
          --set router.image.tag=${IMAGE_TAG} \
          --set gateway.image.repository=${REGISTRY_PREFIX}/gateway \
          --set gateway.image.tag=${IMAGE_TAG} \
          --set config.templates.odgmImageOverride=${REGISTRY_PREFIX}/odgm:${IMAGE_TAG}"
  fi
}

get_pm_image_args(){
  if [[ -n "$REGISTRY_PREFIX" ]]; then
    echo "--set image.repository=${REGISTRY_PREFIX}/pm \
          --set image.tag=${IMAGE_TAG}"
  fi
}

# Computed names
crm_release_name(){ local i="$1"; echo "${CRM_RELEASE_PREFIX}-${i}"; }
crm_namespace(){ local i="$1"; echo "${CRM_NS_PREFIX}-${i}"; }

# Values files
ensure_crm_values_file(){
  local i="$1"; local path="$CHARTS_DIR/examples/${VALUES_PREFIX}crm-${i}.yaml"
  if [[ ! -f "$path" ]]; then
    log "Creating missing CRM values file: examples/${VALUES_PREFIX}crm-${i}.yaml"
    cat >"$path" <<YAML
# CRM ${i} values (override as needed)
{}
YAML
  fi
  echo "$path"
}
ensure_pm_values_file(){
  local path="$CHARTS_DIR/examples/${VALUES_PREFIX}pm.yaml"
  if [[ ! -f "$path" ]]; then
    log "Creating missing PM values file: examples/${VALUES_PREFIX}pm.yaml"
    echo '{}' >"$path"
  fi
  echo "$path"
}

# Show how to connect with oprc-cli after deploy
print_pm_access_command(){
  # Derive service name similar to Helm fullname template logic
  local svc
  if [[ "$PM_RELEASE" == *oprc-pm* ]]; then
    svc="$PM_RELEASE"
  else
    svc="${PM_RELEASE}-oprc-pm"
  fi
  local base_url=""
  if [[ -n "${PM_DOMAIN}" ]]; then
    base_url="http://${PM_DOMAIN}"
  else
    # Try detect NodePort
    local svc_type
    svc_type=$(kubectl get svc "$svc" -n "$PM_NS" -o jsonpath='{.spec.type}' 2>/dev/null || echo "")
    if [[ "$svc_type" == "NodePort" ]]; then
      local node_port
      node_port=$(kubectl get svc "$svc" -n "$PM_NS" -o jsonpath='{.spec.ports[0].nodePort}' 2>/dev/null || echo "")
      if [[ -n "$node_port" ]]; then
        base_url="http://localhost:${node_port}"
      fi
    fi
  fi
  if [[ -z "$base_url" ]]; then
    # Fallback cluster-internal (service ClusterIP) URL
    base_url="http://${svc}.${PM_NS}.svc.cluster.local:8080"
  fi
  echo
  log "To set CLI context for PM:" 
  echo "  oprc-cli ctx set --pm ${base_url}" 
  echo
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
  
  local set_args=(
    --set crd.create=false
    --set config.namespace="$ns"
  )
  if [[ -n "$IMAGE_REGISTRY" ]]; then
    set_args+=(--set image.repository="${IMAGE_REGISTRY}/oaas-rs/crm")
  fi

  # shellcheck disable=SC2046
  helm upgrade --install "$rel" "$CHARTS_DIR/oprc-crm" \
    --namespace "$ns" --create-namespace \
    --values "$values_file" \
    "${set_args[@]}" \
    $(get_crm_image_args) \
    --wait
}

install_or_upgrade_pm(){
  local cmd="$1"
  ensure_ns "$PM_NS"
  local crm1_rel="$(crm_release_name 1)"
  local crm1_ns="$(crm_namespace 1)"
  local crm1_svc_fqdn="http://${crm1_rel}-oprc-crm.${crm1_ns}.svc.cluster.local:8088"
  if [[ -n "$PM_DOMAIN" ]]; then
    log "${cmd^} PM release $PM_RELEASE in $PM_NS (CRM default: $crm1_svc_fqdn, Ingress host: $PM_DOMAIN)"
  else
    log "${cmd^} PM release $PM_RELEASE in $PM_NS (CRM default: $crm1_svc_fqdn)"
  fi
  local pm_values
  pm_values="$(ensure_pm_values_file)"
  # Build dynamic --set flags
  local set_args=(
    --set config.crm.default.url="$crm1_svc_fqdn"
  )
  if [[ -n "$IMAGE_REGISTRY" ]]; then
    set_args+=(--set image.repository="${IMAGE_REGISTRY}/oaas-rs/pm")
  fi
  if [[ -n "$PM_DOMAIN" ]]; then
    # Enable ingress and set host with a basic path
    set_args+=(
      --set ingress.enabled=true
      --set ingress.hosts[0].host="$PM_DOMAIN"
      --set ingress.hosts[0].paths[0].path="/"
      --set ingress.hosts[0].paths[0].pathType=Prefix
    )
  fi
  # shellcheck disable=SC2046
  helm upgrade --install "$PM_RELEASE" "$CHARTS_DIR/oprc-pm" \
    --namespace "$PM_NS" --create-namespace \
    --values "$pm_values" \
    "${set_args[@]}" \
    $(get_pm_image_args) \
    --wait
}

# Create a single multi-document YAML representing the deployment (no Helm release state)
bundle_render(){
  ensure_tools
  local out="$BUNDLE_OUTPUT"
  rm -f "$out"
  if [[ "$BUNDLE_SINGLE" == true ]]; then
    log "Bundling (single namespace mode) into $out (PM & CRM in namespace: $PM_NS)"
  else
    log "Bundling manifests into $out (CRMs: $CRM_COUNT, PM ns: $PM_NS)"
  fi
  if [[ "$BUNDLE_SKIP_CRD" != true ]]; then
    log "Including generated CRD first"
    cargo run -q -p oprc-crm --bin crdgen >>"$out"
    echo -e "\n---" >>"$out"
  else
    warn "Skipping CRD generation (bundle-skip-crd=true)"
  fi
  if [[ "$BUNDLE_SINGLE" == true ]]; then
    # Single namespace: only PM_NS; one CRM release using examples/crm-single.yaml; PM using pm-single.yaml
    cat >>"$out" <<YAML
apiVersion: v1
kind: Namespace
metadata:
  name: ${PM_NS}
---
YAML
    local crm_values_file="$CHARTS_DIR/examples/crm-single.yaml"
    local pm_values_file="$CHARTS_DIR/examples/pm-single.yaml"
    if [[ ! -f "$crm_values_file" ]]; then warn "crm-single.yaml not found; proceeding without explicit values"; fi
    if [[ ! -f "$pm_values_file" ]]; then warn "pm-single.yaml not found; proceeding without explicit values"; fi
    local rel="$(crm_release_name 1)" ns="$PM_NS"
    log "Rendering CRM (single) $rel in $ns"
    if [[ -f "$crm_values_file" ]]; then
      # shellcheck disable=SC2046
      helm template "$rel" "$CHARTS_DIR/oprc-crm" \
        --namespace "$ns" \
        --values "$crm_values_file" \
        --set crd.create=false \
        --set config.namespace="$ns" \
        $(get_crm_image_args) >>"$out"
    else
      # shellcheck disable=SC2046
      helm template "$rel" "$CHARTS_DIR/oprc-crm" \
        --namespace "$ns" \
        --set crd.create=false \
        --set config.namespace="$ns" \
        $(get_crm_image_args) >>"$out"
    fi
    echo -e "\n---" >>"$out"
    # PM release
    log "Rendering PM (single) $PM_RELEASE in $PM_NS"
    if [[ -f "$pm_values_file" ]]; then
      # shellcheck disable=SC2046
      helm template "$PM_RELEASE" "$CHARTS_DIR/oprc-pm" \
        --namespace "$PM_NS" \
        --values "$pm_values_file" \
        $(get_pm_image_args) >>"$out"
    else
      # derive default CRM url referencing same ns
      local crm1_url="http://$(crm_release_name 1)-oprc-crm.${PM_NS}.svc.cluster.local:8088"
      # shellcheck disable=SC2046
      helm template "$PM_RELEASE" "$CHARTS_DIR/oprc-pm" \
        --namespace "$PM_NS" \
        --set config.crm.default.url="$crm1_url" \
        $(get_pm_image_args) >>"$out"
    fi
  else
    # Multi-namespace path (original behavior)
    { cat <<YAML
apiVersion: v1
kind: Namespace
metadata:
  name: ${PM_NS}
---
YAML
      for i in $(seq 1 "$CRM_COUNT"); do
        echo "apiVersion: v1"; echo "kind: Namespace"; echo "metadata:"; echo "  name: ${CRM_NS_PREFIX}-${i}"; echo '---'
      done
    } >>"$out"
    for i in $(seq 1 "$CRM_COUNT"); do
      local rel="$(crm_release_name "$i")" ns="$(crm_namespace "$i")"
      log "Rendering CRM $i ($rel in $ns)"
      # shellcheck disable=SC2046
      helm template "$rel" "$CHARTS_DIR/oprc-crm" \
        --namespace "$ns" \
        --set crd.create=false \
        --set config.namespace="$ns" \
        $(get_crm_image_args) >>"$out"
      echo -e "\n---" >>"$out"
    done
    local crm1_rel="$(crm_release_name 1)" crm1_ns="$(crm_namespace 1)"
    local crm1_url="http://${crm1_rel}-oprc-crm.${crm1_ns}.svc.cluster.local:8088"
    log "Rendering PM release $PM_RELEASE (namespace $PM_NS) default CRM: $crm1_url"
    local pm_args=(
      template "$PM_RELEASE" "$CHARTS_DIR/oprc-pm" \
        --namespace "$PM_NS" \
        --set config.crm.default.url="$crm1_url"
    )
    if [[ -n "$PM_DOMAIN" ]]; then
      pm_args+=(
        --set ingress.enabled=true \
        --set ingress.hosts[0].host="$PM_DOMAIN" \
        --set ingress.hosts[0].paths[0].path="/" \
        --set ingress.hosts[0].paths[0].pathType=Prefix
      )
    fi
    # shellcheck disable=SC2046
    helm "${pm_args[@]}" $(get_pm_image_args) >>"$out"
  fi
  log "Bundle complete: $out"
  echo "Apply with: kubectl apply -f $out"
  if grep -q 'app.kubernetes.io/managed-by: Helm' "$out" 2>/dev/null; then
    warn "Bundle contains Helm labels/annotations (expected). They are inert when applying raw manifests."
  fi
}

uninstall_release(){
  local rel="$1" ns="$2"; log "Uninstalling $rel from $ns"; helm uninstall "$rel" -n "$ns" >/dev/null 2>&1 || true
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
    log "Deploy completed. PM namespace: $PM_NS; CRMs: ${CRM_NS_PREFIX}-1..${CRM_NS_PREFIX}-${CRM_COUNT}"
    print_pm_access_command ;;

  undeploy|uninstall)
    ensure_tools
    log "Deleting all ClassRuntime resources..."
    kubectl delete classruntimes --all --all-namespaces --ignore-not-found --wait=true 2>/dev/null || true

    log "Deleting ClassRuntime CRD..."
    kubectl delete crd classruntimes.oaas.io --ignore-not-found

    uninstall_release "$PM_RELEASE" "$PM_NS"
    for i in $(seq "$CRM_COUNT" -1 1); do
      rel="$(crm_release_name "$i")"; ns="$(crm_namespace "$i")"
      uninstall_release "$rel" "$ns"
    done
    
    delete_namespace_if_requested "$PM_NS"
    for i in $(seq "$CRM_COUNT" -1 1); do
      ns="$(crm_namespace "$i")"; delete_namespace_if_requested "$ns"
    done
    log "Undeploy completed." ;;

  status)
    ensure_tools
  status_ns "$PM_NS"
  for i in $(seq 1 "$CRM_COUNT"); do ns="$(crm_namespace "$i")"; status_ns "$ns"; done ;;

  bundle)
    bundle_render ;;

  *)
    show_help; error "Unknown action: $ACTION" ;;
esac

log "Action '$ACTION' completed."

