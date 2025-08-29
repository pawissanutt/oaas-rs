#!/bin/bash
# OaaS Deployment Script
# Usage: ./deploy.sh [environment] [action] [flags]
# Examples:
#   ./deploy.sh dev install
#   ./deploy.sh nodeport install  # External access via NodePort
#   ./deploy.sh prod install --operators  # With Prometheus + Knative operators
#   ./deploy.sh dev install --prometheus  # With Prometheus operator only
#   ./deploy.sh dev install --knative     # With Knative operator only
#   ./deploy.sh prod upgrade
#   ./deploy.sh staging uninstall

set -e

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Function to show help
show_help() {
    cat << EOF
OaaS Deployment Script

USAGE:
    ./deploy.sh [environment] [action] [flags]

ENVIRONMENTS:
    dev, development    Development environment with debug settings
    prod, production    Production environment with resource limits
    staging             Staging environment with combined configuration
    nodeport            NodePort access for external connectivity

ACTIONS:
    install             Install OaaS components
    upgrade             Upgrade existing OaaS deployment
    uninstall           Remove OaaS components (keeps operators for reuse)
    uninstall --operators   Complete removal including all operators and etcd
    status              Show deployment status and resources
    test                Run health checks on deployed components
    help                Show this help message

FLAGS:
    --operators         Enable both Prometheus and Knative operators
    --prometheus        Enable Prometheus operator for monitoring
    --knative           Enable Knative operator for serverless runtime

EXAMPLES:
    ./deploy.sh dev install                    # Basic development deployment
    ./deploy.sh prod install --operators       # Production with all operators
    ./deploy.sh dev install --prometheus       # Development with monitoring
    ./deploy.sh nodeport install --knative     # NodePort with serverless
    ./deploy.sh staging status                 # Check staging status
    ./deploy.sh prod upgrade --operators       # Upgrade with operators
    ./deploy.sh dev uninstall                  # Remove OaaS but keep operators
    ./deploy.sh prod uninstall --operators     # Complete removal with operators

OPERATOR FEATURES:
    Prometheus Operator:
        - Automatic ServiceMonitor creation
        - PrometheusRule for alerting
        - Metrics collection setup

    Knative Operator:
        - Serverless runtime (Knative Serving)
        - Kourier ingress controller
        - Magic DNS for development
        - Note: Knative Eventing removed (unnecessary)

    Note: When operators are enabled, 'helm dependency build' is automatically 
    run to fetch required operator charts (kube-prometheus-stack, knative-operator)
    
    Note: PM chart dependencies (etcd) are automatically built during deployment
    
    Note: Operators are checked for existing installations and reused when available.
    Uninstall keeps operators for reuse by other deployments.

For more information, see: k8s/charts/README.md
EOF
}

# Function to configure oprc-cli or ocli
configure_oprc_cli() {
    local environment="$1"
    local namespace="$2"
    
    log "Detecting OaaS CLI tools..."
    
    # Check for oprc-cli or ocli
    local cli_command=""
    if command -v oprc-cli &>/dev/null; then
        cli_command="oprc-cli"
        log "Found oprc-cli"
    elif command -v ocli &>/dev/null; then
        cli_command="ocli"
        log "Found ocli"
    else
        log "No OaaS CLI tool found (oprc-cli or ocli)."
        log "To install oprc-cli, run: cargo install --path tools/oprc-cli"
        cli_command="oprc-cli"  # Use as default for command generation
    fi
    
    log "Generating CLI configuration command for $environment environment..."
    
    # Get service information
    local pm_service_type pm_service_port pm_node_port
    local crm_service_type crm_service_port crm_node_port
    local router_service_type router_service_port router_node_port router_host_port
    
    pm_service_type=$(kubectl get svc "oaas-pm-${environment}-oprc-pm" -n "$namespace" -o jsonpath='{.spec.type}' 2>/dev/null)
    pm_service_port=$(kubectl get svc "oaas-pm-${environment}-oprc-pm" -n "$namespace" -o jsonpath='{.spec.ports[0].port}' 2>/dev/null)
    pm_node_port=$(kubectl get svc "oaas-pm-${environment}-oprc-pm" -n "$namespace" -o jsonpath='{.spec.ports[0].nodePort}' 2>/dev/null)
    
    crm_service_type=$(kubectl get svc "oaas-crm-${environment}-oprc-crm" -n "$namespace" -o jsonpath='{.spec.type}' 2>/dev/null)
    crm_service_port=$(kubectl get svc "oaas-crm-${environment}-oprc-crm" -n "$namespace" -o jsonpath='{.spec.ports[0].port}' 2>/dev/null)
    crm_node_port=$(kubectl get svc "oaas-crm-${environment}-oprc-crm" -n "$namespace" -o jsonpath='{.spec.ports[0].nodePort}' 2>/dev/null)
    
    router_service_type=$(kubectl get svc "oaas-crm-${environment}-router" -n "$namespace" -o jsonpath='{.spec.type}' 2>/dev/null)
    router_service_port=$(kubectl get svc "oaas-crm-${environment}-router" -n "$namespace" -o jsonpath='{.spec.ports[0].port}' 2>/dev/null)
    router_node_port=$(kubectl get svc "oaas-crm-${environment}-router" -n "$namespace" -o jsonpath='{.spec.ports[0].nodePort}' 2>/dev/null)
    
    # Check for router hostPort
    router_host_port=$(kubectl get pod -l app=router -n "$namespace" -o jsonpath='{.items[0].spec.containers[0].ports[?(@.name=="zenoh")].hostPort}' 2>/dev/null)
    
    # Get node IP for NodePort services
    local node_ip
    node_ip=$(kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type=="InternalIP")].address}' 2>/dev/null)
    if [[ -z "$node_ip" ]]; then
        node_ip="localhost"
    fi
    
    # Determine endpoints based on service types
    local pm_url="http://localhost:8080"
    local crm_url="http://localhost:8088"
    local router_url="tcp/localhost:17447"
    
    # Configure PM endpoint
    if [[ "$pm_service_type" == "NodePort" && -n "$pm_node_port" ]]; then
        pm_url="http://$node_ip:$pm_node_port"
    elif [[ "$pm_service_type" == "ClusterIP" ]]; then
        pm_url="http://localhost:8080  # Use: kubectl port-forward svc/oaas-pm-${environment}-oprc-pm 8080:${pm_service_port} -n $namespace"
    fi
    
    # Configure CRM endpoint
    if [[ "$crm_service_type" == "NodePort" && -n "$crm_node_port" ]]; then
        crm_url="http://$node_ip:$crm_node_port"
    elif [[ "$crm_service_type" == "ClusterIP" ]]; then
        crm_url="http://localhost:8088  # Use: kubectl port-forward svc/oaas-crm-${environment}-oprc-crm 8088:${crm_service_port} -n $namespace"
    fi
    
    # Configure Router endpoint (Zenoh)
    if [[ -n "$router_host_port" ]]; then
        router_url="tcp/$node_ip:$router_host_port"
    elif [[ "$router_service_type" == "NodePort" && -n "$router_node_port" ]]; then
        router_url="tcp/$node_ip:$router_node_port"
    elif [[ "$router_service_type" == "ClusterIP" ]]; then
        router_url="tcp/localhost:17447  # Use: kubectl port-forward svc/oaas-crm-${environment}-router 17447:${router_service_port} -n $namespace"
    fi
    
    # Check for ingress
    local pm_ingress crm_ingress
    pm_ingress=$(kubectl get ingress -n "$namespace" -l app.kubernetes.io/name=oprc-pm -o jsonpath='{.items[0].spec.rules[0].host}' 2>/dev/null)
    crm_ingress=$(kubectl get ingress -n "$namespace" -l app.kubernetes.io/name=oprc-crm -o jsonpath='{.items[0].spec.rules[0].host}' 2>/dev/null)
    
    if [[ -n "$pm_ingress" ]]; then
        pm_url="https://$pm_ingress"
    fi
    if [[ -n "$crm_ingress" ]]; then
        crm_url="https://$crm_ingress"
    fi
    
    # Generate the CLI configuration command
    echo ""
    echo "=== CLI Configuration Command ==="
    echo "Run the following command to configure your OaaS CLI:"
    echo ""
    echo "$cli_command context set $environment \\"
    echo "  --pm \"$pm_url\" \\"
    echo "  --zenoh-peer \"$router_url\""
    echo ""
    
    # Show port-forward commands if needed
    if [[ "$pm_service_type" == "ClusterIP" || "$crm_service_type" == "ClusterIP" || ("$router_service_type" == "ClusterIP" && -z "$router_host_port") ]]; then
        echo "=== Required Port Forwarding ==="
        echo "For ClusterIP services, run these port-forward commands in separate terminals:"
        echo ""
        
        if [[ "$pm_service_type" == "ClusterIP" ]]; then
            echo "kubectl port-forward svc/oaas-pm-${environment}-oprc-pm 8080:${pm_service_port} -n $namespace"
        fi
        
        if [[ "$crm_service_type" == "ClusterIP" ]]; then
            echo "kubectl port-forward svc/oaas-crm-${environment}-oprc-crm 8088:${crm_service_port} -n $namespace"
        fi
        
        if [[ "$router_service_type" == "ClusterIP" && -z "$router_host_port" ]]; then
            echo "kubectl port-forward svc/oaas-crm-${environment}-router 17447:${router_service_port} -n $namespace"
        fi
        echo ""
    fi
    
    # Show usage examples
    echo "=== Usage Examples ==="
    echo "$cli_command ctx use $environment"
    echo "$cli_command package list"
    echo "$cli_command deployment list"
    echo ""
}

# Function to check operator status
check_operator_status() {
    local prometheus_operator=false
    local knative_operator=false
    local prometheus_installed=false
    local knative_installed=false
    
    # Check if Prometheus Operator CRDs exist
    if kubectl get crd servicemonitors.monitoring.coreos.com &>/dev/null; then
        prometheus_operator=true
    fi
    
    # Check if Knative Operator CRDs exist
    if kubectl get crd knativeservings.operator.knative.dev &>/dev/null; then
        knative_operator=true
    fi
    
    # Check if Prometheus is actually installed
    if kubectl get prometheus --all-namespaces &>/dev/null; then
        prometheus_installed=true
    fi
    
    # Check if Knative Serving is installed
    if kubectl get knativeservings --all-namespaces &>/dev/null; then
        knative_installed=true
    fi
    
    echo "$prometheus_operator $knative_operator $prometheus_installed $knative_installed"
}

# Handle help or no arguments
if [[ $# -eq 0 || "$1" == "help" || "$1" == "--help" || "$1" == "-h" ]]; then
    show_help
    exit 0
fi

ENVIRONMENT=${1:-dev}
ACTION=${2:-install}
CHARTS_DIR="$(dirname "$0")"
NAMESPACE="oaas-${ENVIRONMENT}"

# Parse flags
ENABLE_PROMETHEUS=false
ENABLE_KNATIVE=false
ENABLE_OPERATORS=false

# Process remaining arguments as flags
shift 2 2>/dev/null || true
while [[ $# -gt 0 ]]; do
    case $1 in
        --operators)
            ENABLE_OPERATORS=true
            ENABLE_PROMETHEUS=true
            ENABLE_KNATIVE=true
            shift
            ;;
        --prometheus)
            ENABLE_PROMETHEUS=true
            shift
            ;;
        --knative)
            ENABLE_KNATIVE=true
            shift
            ;;
        *)
            echo "Unknown flag: $1"
            echo "Available flags: --operators, --prometheus, --knative"
            exit 1
            ;;
    esac
done

# Function to get values file and build Helm arguments
get_helm_args() {
    local component=$1
    local values_file=""
    local extra_args=""
    
    # Determine base values file
    case $ENVIRONMENT in
        dev|development)
            values_file="$CHARTS_DIR/examples/${component}-development.yaml"
            ;;
        prod|production)
            values_file="$CHARTS_DIR/examples/${component}-production.yaml"
            ;;
        staging)
            values_file="$CHARTS_DIR/examples/combined-deployment.yaml"
            ;;
        nodeport)
            values_file="$CHARTS_DIR/examples/${component}-nodeport.yaml"
            ;;
    esac
    
    # Add component-specific arguments
    if [[ "$component" == "crm" ]]; then
        # Configure CRM to use the correct namespace
        local namespace="oaas-${ENVIRONMENT}"
        extra_args="$extra_args --set config.namespace=${namespace}"
        
        # Add operator-specific arguments for CRM
        if [[ "$ENABLE_OPERATORS" == "true" ]]; then
            # Use the comprehensive operators example
            values_file="$CHARTS_DIR/examples/crm-with-operators.yaml"
        else
            # Add individual operator flags
            if [[ "$ENABLE_PROMETHEUS" == "true" ]]; then
                extra_args="$extra_args --set prometheus.operator.enabled=true"
                extra_args="$extra_args --set prometheus.serviceMonitor.enabled=true"
                extra_args="$extra_args --set prometheus.prometheusRule.enabled=true"
            fi
            
            if [[ "$ENABLE_KNATIVE" == "true" ]]; then
                extra_args="$extra_args --set knative.operator.enabled=true"
                extra_args="$extra_args --set knative.operator.serving.enabled=true"
                extra_args="$extra_args --set knative.operator.serving.ingress.kourier.enabled=true"
                extra_args="$extra_args --set knative.operator.serving.dns.magic=true"
            fi
        fi
    elif [[ "$component" == "pm" ]]; then
        # Configure PM to use the correct CRM service URL
        local crm_service_name="oaas-crm-${ENVIRONMENT}-oprc-crm"
        extra_args="$extra_args --set config.crm.default.url=http://${crm_service_name}:8088"
    fi
    
    echo "--values $values_file $extra_args"
}

# Validate environment
case $ENVIRONMENT in
    dev|development|prod|production|staging|nodeport)
        # Valid environment
        ;;
    *)
        error "Unknown environment: $ENVIRONMENT. Use dev, staging, prod, or nodeport."
        ;;
esac

# Check if Helm is installed
if ! command -v helm &> /dev/null; then
    error "Helm is not installed. Please install Helm 3.0+ and try again."
fi

# Check if kubectl is installed and connected
if ! command -v kubectl &> /dev/null; then
    error "kubectl is not installed. Please install kubectl and try again."
fi

if ! kubectl cluster-info &> /dev/null; then
    error "Cannot connect to Kubernetes cluster. Please check your kubeconfig."
fi

# Create namespace if it doesn't exist
if ! kubectl get namespace "$NAMESPACE" &> /dev/null; then
    log "Creating namespace: $NAMESPACE"
    kubectl create namespace "$NAMESPACE"
fi

# Create knative-serving namespace if Knative is enabled
if [[ "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
    if ! kubectl get namespace "knative-serving" &> /dev/null; then
        log "Creating Knative namespace: knative-serving"
        kubectl create namespace "knative-serving"
    fi
fi

case $ACTION in
    install)
        log "Installing OaaS components in $ENVIRONMENT environment..."
        
        # Check existing operator status
        operator_status=($(check_operator_status))
        prometheus_operator=${operator_status[0]}
        knative_operator=${operator_status[1]}
        prometheus_installed=${operator_status[2]}
        knative_installed=${operator_status[3]}
        
        if [[ "$ENABLE_OPERATORS" == "true" ]]; then
            log "Operators enabled: Prometheus + Knative"
            [[ "$prometheus_installed" == "true" ]] && log "Prometheus Operator already installed - reusing existing installation"
            [[ "$knative_installed" == "true" ]] && log "Knative Operator already installed - reusing existing installation"
        elif [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" ]]; then
            log "Operators enabled: Prometheus=$ENABLE_PROMETHEUS, Knative=$ENABLE_KNATIVE"
            [[ "$ENABLE_PROMETHEUS" == "true" && "$prometheus_installed" == "true" ]] && log "Prometheus Operator already installed - reusing existing installation"
            [[ "$ENABLE_KNATIVE" == "true" && "$knative_installed" == "true" ]] && log "Knative Operator already installed - reusing existing installation"
        fi
        
        # Determine if we need to set up operators
        need_operator_setup=false
        if [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
            if [[ ("$ENABLE_PROMETHEUS" == "true" || "$ENABLE_OPERATORS" == "true") && "$prometheus_installed" == "false" ]] || 
               [[ ("$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true") && "$knative_installed" == "false" ]]; then
                need_operator_setup=true
            fi
        fi
        
        # Build chart dependencies based on operator setup needs
        if [[ "$need_operator_setup" == "true" ]]; then
            log "Setting up operators that are not already installed..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts >/dev/null 2>&1 || true
            helm repo add knative https://knative.github.io/operator >/dev/null 2>&1 || true
            helm repo update >/dev/null 2>&1 || true
            
            log "Building Helm chart dependencies for operators..."
            cd "$CHARTS_DIR/oprc-crm"
            helm dependency build
            if [ $? -ne 0 ]; then
                error "Failed to build CRM chart dependencies"
            fi
            cd "$CHARTS_DIR"
        elif [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
            log "All requested operators already installed - ensuring chart dependencies..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts >/dev/null 2>&1 || true
            helm repo add knative https://knative.github.io/operator >/dev/null 2>&1 || true
            helm repo update >/dev/null 2>&1 || true
            
            log "Building CRM chart dependencies for template compatibility..."
            cd "$CHARTS_DIR/oprc-crm"
            helm dependency build
            cd "$CHARTS_DIR"
        fi
        
        # Build PM chart dependencies
        log "Adding Helm repositories for PM dependencies..."
        helm repo add bitnami https://charts.bitnami.com/bitnami >/dev/null 2>&1 || true
        helm repo update >/dev/null 2>&1 || true
        
        log "Building PM chart dependencies..."
        cd "$CHARTS_DIR/oprc-pm"
        helm dependency build
        if [ $? -ne 0 ]; then
            error "Failed to build PM chart dependencies"
        fi
        cd "$CHARTS_DIR"
        
        # Install CRM first
        log "Installing CRM (Class Runtime Manager)..."
        CRM_ARGS=$(get_helm_args "crm")
        eval helm install "oaas-crm-${ENVIRONMENT}" "$CHARTS_DIR/oprc-crm" \
            --namespace "$NAMESPACE" \
            $CRM_ARGS \
            --wait
        
        # Wait a bit for CRM to be ready
        log "Waiting for CRM to be ready..."
        kubectl wait --for=condition=available deployment/oaas-crm-${ENVIRONMENT}-oprc-crm \
            --namespace "$NAMESPACE" --timeout=300s
        
        # Install PM
        log "Installing PM (Package Manager)..."
        PM_ARGS=$(get_helm_args "pm")
        eval helm install "oaas-pm-${ENVIRONMENT}" "$CHARTS_DIR/oprc-pm" \
            --namespace "$NAMESPACE" \
            $PM_ARGS \
            --wait
        
        log "OaaS installation completed successfully!"
        
        # Configure CLI if available
        configure_oprc_cli "$ENVIRONMENT" "$NAMESPACE"
        ;;
        
    upgrade)
        log "Upgrading OaaS components in $ENVIRONMENT environment..."
        
        # Check existing operator status
        operator_status=($(check_operator_status))
        prometheus_operator=${operator_status[0]}
        knative_operator=${operator_status[1]}
        prometheus_installed=${operator_status[2]}
        knative_installed=${operator_status[3]}
        
        if [[ "$ENABLE_OPERATORS" == "true" ]]; then
            log "Operators enabled: Prometheus + Knative"
            [[ "$prometheus_installed" == "true" ]] && log "Prometheus Operator already installed - reusing existing installation"
            [[ "$knative_installed" == "true" ]] && log "Knative Operator already installed - reusing existing installation"
        elif [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" ]]; then
            log "Operators enabled: Prometheus=$ENABLE_PROMETHEUS, Knative=$ENABLE_KNATIVE"
            [[ "$ENABLE_PROMETHEUS" == "true" && "$prometheus_installed" == "true" ]] && log "Prometheus Operator already installed - reusing existing installation"
            [[ "$ENABLE_KNATIVE" == "true" && "$knative_installed" == "true" ]] && log "Knative Operator already installed - reusing existing installation"
        fi
        
        # Determine if we need to set up operators
        need_operator_setup=false
        if [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
            if [[ ("$ENABLE_PROMETHEUS" == "true" || "$ENABLE_OPERATORS" == "true") && "$prometheus_installed" == "false" ]] || 
               [[ ("$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true") && "$knative_installed" == "false" ]]; then
                need_operator_setup=true
            fi
        fi
        
        # Build chart dependencies based on operator setup needs
        if [[ "$need_operator_setup" == "true" ]]; then
            log "Setting up operators that are not already installed..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts >/dev/null 2>&1 || true
            helm repo add knative https://knative.github.io/operator >/dev/null 2>&1 || true
            helm repo update >/dev/null 2>&1 || true
            
            log "Building Helm chart dependencies for operators..."
            cd "$CHARTS_DIR/oprc-crm"
            helm dependency build
            if [ $? -ne 0 ]; then
                error "Failed to build CRM chart dependencies"
            fi
            cd "$CHARTS_DIR"
        elif [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
            log "All requested operators already installed - ensuring chart dependencies..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts >/dev/null 2>&1 || true
            helm repo add knative https://knative.github.io/operator >/dev/null 2>&1 || true
            helm repo update >/dev/null 2>&1 || true
            
            log "Building CRM chart dependencies for template compatibility..."
            cd "$CHARTS_DIR/oprc-crm"
            helm dependency build
            cd "$CHARTS_DIR"
        fi
        
        # Build PM chart dependencies
        log "Adding Helm repositories for PM dependencies..."
        helm repo add bitnami https://charts.bitnami.com/bitnami >/dev/null 2>&1 || true
        helm repo update >/dev/null 2>&1 || true
        
        log "Building PM chart dependencies..."
        cd "$CHARTS_DIR/oprc-pm"
        helm dependency build
        if [ $? -ne 0 ]; then
            error "Failed to build PM chart dependencies"
        fi
        cd "$CHARTS_DIR"
        
        # Upgrade CRM
        log "Upgrading CRM..."
        CRM_ARGS=$(get_helm_args "crm")
        eval helm upgrade "oaas-crm-${ENVIRONMENT}" "$CHARTS_DIR/oprc-crm" \
            --namespace "$NAMESPACE" \
            $CRM_ARGS \
            --wait
        
        # Upgrade PM
        log "Upgrading PM..."
        PM_ARGS=$(get_helm_args "pm")
        eval helm upgrade "oaas-pm-${ENVIRONMENT}" "$CHARTS_DIR/oprc-pm" \
            --namespace "$NAMESPACE" \
            $PM_ARGS \
            --wait
        
        log "OaaS upgrade completed successfully!"
        ;;
        
    uninstall)
        warn "Uninstalling OaaS components from $ENVIRONMENT environment..."
        
        if [[ "$ENABLE_OPERATORS" == "true" || "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" ]]; then
            log "Operators flag detected - performing complete cleanup including operators"
        else
            log "Note: Operators (Prometheus, Knative) will be kept for reuse"
        fi
        
        # Uninstall PM first (to avoid dependency issues)
        log "Uninstalling PM..."
        helm uninstall "oaas-pm-${ENVIRONMENT}" --namespace "$NAMESPACE" || warn "PM release not found"
        
        # Uninstall CRM
        log "Uninstalling CRM..."
        helm uninstall "oaas-crm-${ENVIRONMENT}" --namespace "$NAMESPACE" || warn "CRM release not found"
        
        # Remove operators if flag is specified
        if [[ "$ENABLE_OPERATORS" == "true" || "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" ]]; then
            log "Removing operators and their components..."
            
            # Remove Knative components
            if [[ "$ENABLE_KNATIVE" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
                log "Removing Knative Serving resources..."
                kubectl delete knativeservings --all --all-namespaces &>/dev/null || true
                
                log "Removing Knative Operator..."
                helm uninstall knative-operator --namespace "$NAMESPACE" &>/dev/null || warn "Knative Operator release not found"
                
                log "Cleaning up Knative CRDs..."
                kubectl delete crd -l knative.dev/crd-install=true &>/dev/null || true
                kubectl delete crd -l app.kubernetes.io/name=knative-serving &>/dev/null || true
                kubectl delete crd -l app.kubernetes.io/name=knative-operator &>/dev/null || true
                
                log "Removing Knative namespaces..."
                kubectl delete namespace knative-serving --ignore-not-found=true &>/dev/null || true
                kubectl delete namespace knative-operator --ignore-not-found=true &>/dev/null || true
            fi
            
            # Remove Prometheus components
            if [[ "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_OPERATORS" == "true" ]]; then
                log "Removing Prometheus Operator..."
                helm uninstall kube-prometheus-stack --namespace "$NAMESPACE" &>/dev/null || warn "Prometheus Operator release not found"
            fi
            
            # Remove etcd (PM dependency)
            log "Removing etcd..."
            helm uninstall etcd --namespace "$NAMESPACE" &>/dev/null || warn "etcd release not found"
        fi
        
        if [[ "$ENABLE_OPERATORS" == "true" || "$ENABLE_PROMETHEUS" == "true" || "$ENABLE_KNATIVE" == "true" ]]; then
            log "Complete OaaS and operators uninstallation completed!"
        else
            log "OaaS uninstallation completed! Operators remain available for reuse."
        fi
        ;;
        
    status)
        log "Checking OaaS status in $ENVIRONMENT environment..."
        
        echo "=== Helm Releases ==="
        helm list --namespace "$NAMESPACE"
        
        echo "=== Deployments ==="
        kubectl get deployments --namespace "$NAMESPACE"
        
        echo "=== Services ==="
        kubectl get services --namespace "$NAMESPACE"
        
        echo "=== Custom Resources ==="
    kubectl get classruntimes.oaas.io --namespace "$NAMESPACE" 2>/dev/null || echo "No ClassRuntimes found"
        
        echo "=== Operator Status ==="
        # Check for Prometheus Operator resources
        kubectl get servicemonitors.monitoring.coreos.com --namespace "$NAMESPACE" 2>/dev/null && echo "Prometheus ServiceMonitor found" || echo "No Prometheus ServiceMonitor"
        kubectl get prometheusrules.monitoring.coreos.com --namespace "$NAMESPACE" 2>/dev/null && echo "Prometheus Rules found" || echo "No Prometheus Rules"
        
        # Check for Knative resources
        kubectl get knativeservings.operator.knative.dev --all-namespaces 2>/dev/null && echo "Knative Serving found" || echo "No Knative Serving"
        
        echo "=== Pods ==="
        kubectl get pods --namespace "$NAMESPACE"
        ;;
        
    test)
        log "Testing OaaS deployment in $ENVIRONMENT environment..."
        
        # Test CRM health
        log "Testing CRM health..."
        kubectl port-forward "svc/oaas-crm-${ENVIRONMENT}-oprc-crm" 8088:8088 --namespace "$NAMESPACE" &
        PORT_FORWARD_PID=$!
        sleep 5
        
        if curl -f http://localhost:8088/health &> /dev/null; then
            log "CRM health check passed"
        else
            error "CRM health check failed"
        fi
        
        kill $PORT_FORWARD_PID &> /dev/null || true
        
        # Test PM health
        log "Testing PM health..."
        kubectl port-forward "svc/oaas-pm-${ENVIRONMENT}-oprc-pm" 8080:8080 --namespace "$NAMESPACE" &
        PORT_FORWARD_PID=$!
        sleep 5
        
        if curl -f http://localhost:8080/health &> /dev/null; then
            log "PM health check passed"
        else
            error "PM health check failed"
        fi
        
        kill $PORT_FORWARD_PID &> /dev/null || true
        
        log "All health checks passed!"
        ;;
        
    help|--help|-h)
        show_help
        ;;
        
    *)
        error "Unknown action: $ACTION. Use install, upgrade, uninstall, status, test, or help."
        ;;
esac

log "Action '$ACTION' completed for environment '$ENVIRONMENT'."
