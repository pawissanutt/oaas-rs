# OaaS Deployment Script (PowerShell)
# Usage: .\deploy.ps1 [environmenFLAGS:
    -Operators          Enable both Prometheus and Knative operators (install/upgrade), or remove them (uninstall)
    -Prometheus         Enable Prometheus operator for monitoring (install/upgrade), or remove it (uninstall)
    -Knative            Enable Knative operator for serverless runtime (install/upgrade), or remove it (uninstall)
    -SkipHelm           Skip helm repo update and dependency build (faster for development)
    -Help               Show this help messageion] [flags]
# Examples:
#   .\deploy.ps1 dev install
#   .\deploy.ps1 nodeport install  # External access via NodePort
#   .\deploy.ps1 prod install -Operators  # With Prometheus + Knative operators
#   .\deploy.ps1 dev install -Prometheus  # With Prometheus operator only
#   .\deploy.ps1 dev install -Knative     # With Knative operator only
#   .\deploy.ps1 prod upgrade
#   .\deploy.ps1 staging uninstall

param(
    [Parameter(Position=0)]
    [string]$Environment = "dev",
    
    [Parameter(Position=1)]
    [string]$Action = "install",
    
    [switch]$Operators,
    [switch]$Prometheus,
    [switch]$Knative,
    [switch]$SkipHelm,
    [switch]$Help
)

# Configuration
$ChartsDir = $PSScriptRoot
$Namespace = "oaas-$Environment"

# Process operator flags
$EnablePrometheus = $Prometheus -or $Operators
$EnableKnative = $Knative -or $Operators

# Functions
function Write-Log {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error-Exit {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
    exit 1
}

# Function to show help
function Show-Help {
    Write-Host @"
OaaS Deployment Script (PowerShell)

USAGE:
    .\deploy.ps1 [environment] [action] [flags]

ENVIRONMENTS:
    dev, development    Development environment with debug settings
    prod, production    Production environment with resource limits
    staging             Staging environment with combined configuration
    nodeport            NodePort access for external connectivity

ACTIONS:
    install             Install OaaS components
    upgrade             Upgrade existing OaaS deployment
    uninstall           Remove OaaS components (keeps operators for reuse)
    status              Show deployment status and resources
    test                Run health checks on deployed components
    help                Show this help message

FLAGS:
    -Operators          Enable both Prometheus and Knative operators (install/upgrade), or remove them (uninstall)
    -Prometheus         Enable Prometheus operator for monitoring (install/upgrade), or remove it (uninstall)
    -Knative            Enable Knative operator for serverless runtime (install/upgrade), or remove it (uninstall)
    -SkipHelm           Skip helm repo update and dependency build (faster for development)
    -Help               Show this help message

EXAMPLES:
    .\deploy.ps1 dev install                    # Basic development deployment
    .\deploy.ps1 prod install -Operators        # Production with all operators
    .\deploy.ps1 dev install -Prometheus        # Development with monitoring
    .\deploy.ps1 nodeport install -Knative      # NodePort with serverless
    .\deploy.ps1 staging status                 # Check staging status
    .\deploy.ps1 prod upgrade -Operators        # Upgrade with operators
    .\deploy.ps1 dev uninstall                  # Remove OaaS (keep operators)
    .\deploy.ps1 dev uninstall -Operators       # Remove OaaS and all operators
    .\deploy.ps1 dev install -SkipHelm          # Fast development install

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

For more information, see: k8s\charts\README.md
"@ -ForegroundColor Cyan
}

function Configure-OprcCli {
    param(
        [string]$Environment,
        [string]$Namespace
    )
    
    Write-Log "Detecting OaaS CLI tools..."
    
    # Check for oprc-cli or ocli
    $CliCommand = $null
    if (Get-Command "oprc-cli" -ErrorAction SilentlyContinue) {
        $CliCommand = "oprc-cli"
        Write-Log "Found oprc-cli"
    } elseif (Get-Command "ocli" -ErrorAction SilentlyContinue) {
        $CliCommand = "ocli"
        Write-Log "Found ocli"
    } else {
        Write-Log "No OaaS CLI tool found (oprc-cli or ocli)."
        Write-Log "To install oprc-cli, run: cargo install --path tools/oprc-cli"
        $CliCommand = "oprc-cli"  # Use as default for command generation
    }
    
    Write-Log "Generating CLI configuration command for $Environment environment..."
    
    try {
        # Get PM service info
        $PmServiceType = kubectl get svc "oaas-pm-$Environment-oprc-pm" -n $Namespace -o jsonpath='{.spec.type}' 2>$null
        $PmServicePort = kubectl get svc "oaas-pm-$Environment-oprc-pm" -n $Namespace -o jsonpath='{.spec.ports[0].port}' 2>$null
        $PmNodePort = kubectl get svc "oaas-pm-$Environment-oprc-pm" -n $Namespace -o jsonpath='{.spec.ports[0].nodePort}' 2>$null
        
        # Get CRM service info
        $CrmServiceType = kubectl get svc "oaas-crm-$Environment-oprc-crm" -n $Namespace -o jsonpath='{.spec.type}' 2>$null
        $CrmServicePort = kubectl get svc "oaas-crm-$Environment-oprc-crm" -n $Namespace -o jsonpath='{.spec.ports[0].port}' 2>$null
        $CrmNodePort = kubectl get svc "oaas-crm-$Environment-oprc-crm" -n $Namespace -o jsonpath='{.spec.ports[0].nodePort}' 2>$null
        
        # Get Router service info
        $RouterServiceType = kubectl get svc "oaas-crm-$Environment-router" -n $Namespace -o jsonpath='{.spec.type}' 2>$null
        $RouterServicePort = kubectl get svc "oaas-crm-$Environment-router" -n $Namespace -o jsonpath='{.spec.ports[0].port}' 2>$null
        $RouterNodePort = kubectl get svc "oaas-crm-$Environment-router" -n $Namespace -o jsonpath='{.spec.ports[0].nodePort}' 2>$null
        
        # Check for router hostPort
        $RouterHostPort = kubectl get pod -l app=router -n $Namespace -o jsonpath='{.items[0].spec.containers[0].ports[?(@.name=="zenoh")].hostPort}' 2>$null
        
        # Determine endpoints based on service types
        $PmUrl = "http://localhost:8080"
        $CrmUrl = "http://localhost:8088"
        $RouterUrl = "tcp/localhost:17447"
        $PmComment = ""
        $CrmComment = ""
        $RouterComment = ""
        
        # Get node IP for NodePort services
        $NodeIP = kubectl get nodes -o jsonpath='{.items[0].status.addresses[?(@.type=="InternalIP")].address}' 2>$null
        if (-not $NodeIP) {
            $NodeIP = "localhost"
        }
        
        # Configure PM endpoint
        if ($PmServiceType -eq "NodePort" -and $PmNodePort) {
            $PmUrl = "http://$NodeIP`:$PmNodePort"
        } elseif ($PmServiceType -eq "ClusterIP") {
            $PmUrl = "http://localhost:8080"
            $PmComment = "# Use: kubectl port-forward svc/oaas-pm-$Environment-oprc-pm 8080:$PmServicePort -n $Namespace"
        }
        
        # Configure CRM endpoint
        if ($CrmServiceType -eq "NodePort" -and $CrmNodePort) {
            $CrmUrl = "http://$NodeIP`:$CrmNodePort"
        } elseif ($CrmServiceType -eq "ClusterIP") {
            $CrmUrl = "http://localhost:8088"
            $CrmComment = "# Use: kubectl port-forward svc/oaas-crm-$Environment-oprc-crm 8088:$CrmServicePort -n $Namespace"
        }
        
        # Configure Router endpoint (Zenoh)
        if ($RouterHostPort) {
            $RouterUrl = "tcp/$NodeIP`:$RouterHostPort"
        } elseif ($RouterServiceType -eq "NodePort" -and $RouterNodePort) {
            $RouterUrl = "tcp/$NodeIP`:$RouterNodePort"
        } elseif ($RouterServiceType -eq "ClusterIP") {
            $RouterUrl = "tcp/localhost:17447"
            $RouterComment = "# Use: kubectl port-forward svc/oaas-crm-$Environment-router 17447:$RouterServicePort -n $Namespace"
        }
        
        # Check for ingress
        $PmIngress = kubectl get ingress -n $Namespace -l app.kubernetes.io/name=oprc-pm -o jsonpath='{.items[0].spec.rules[0].host}' 2>$null
        $CrmIngress = kubectl get ingress -n $Namespace -l app.kubernetes.io/name=oprc-crm -o jsonpath='{.items[0].spec.rules[0].host}' 2>$null
        
        if ($PmIngress) {
            $PmUrl = "https://$PmIngress"
        }
        if ($CrmIngress) {
            $CrmUrl = "https://$CrmIngress"
        }
        
        # Generate the CLI configuration command
        Write-Host "`n=== CLI Configuration Command ===" -ForegroundColor Cyan
        Write-Host "Run the following command to configure your OaaS CLI:" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "$CliCommand context set $Environment \\" -ForegroundColor Green
        Write-Host "  --pm `"$PmUrl`" \\" -ForegroundColor Green
        Write-Host "  --zenoh-peer `"$RouterUrl`"" -ForegroundColor Green
        Write-Host ""
        
        # Show comments for ClusterIP services
        if ($PmServiceType -eq "ClusterIP" -and $PmComment) {
            Write-Host "PM: $PmComment" -ForegroundColor Gray
        }
        if ($CrmServiceType -eq "ClusterIP" -and $CrmComment) {
            Write-Host "CRM: $CrmComment" -ForegroundColor Gray
        }
        if ($RouterServiceType -eq "ClusterIP" -and $RouterComment) {
            Write-Host "Router: $RouterComment" -ForegroundColor Gray
        }
        if ($PmServiceType -eq "ClusterIP" -or $CrmServiceType -eq "ClusterIP" -or ($RouterServiceType -eq "ClusterIP" -and -not $RouterHostPort)) {
            Write-Host ""
        }
        
        # Show port-forward commands if needed
        if ($PmServiceType -eq "ClusterIP" -or $CrmServiceType -eq "ClusterIP" -or ($RouterServiceType -eq "ClusterIP" -and -not $RouterHostPort)) {
            Write-Host "=== Required Port Forwarding ===" -ForegroundColor Cyan
            Write-Host "For ClusterIP services, run these port-forward commands in separate terminals:" -ForegroundColor Yellow
            Write-Host ""
            
            if ($PmServiceType -eq "ClusterIP") {
                Write-Host "kubectl port-forward svc/oaas-pm-$Environment-oprc-pm 8080:$PmServicePort -n $Namespace" -ForegroundColor Magenta
            }
            
            if ($CrmServiceType -eq "ClusterIP") {
                Write-Host "kubectl port-forward svc/oaas-crm-$Environment-oprc-crm 8088:$CrmServicePort -n $Namespace" -ForegroundColor Magenta
            }
            
            if ($RouterServiceType -eq "ClusterIP" -and -not $RouterHostPort) {
                Write-Host "kubectl port-forward svc/oaas-crm-$Environment-router 17447:$RouterServicePort -n $Namespace" -ForegroundColor Magenta
            }
            Write-Host ""
        }
        
        # Show usage examples
        Write-Host "=== Usage Examples ===" -ForegroundColor Cyan
        Write-Host "$CliCommand ctx use $Environment" -ForegroundColor Green
        Write-Host "$CliCommand package list" -ForegroundColor Green
        Write-Host "$CliCommand deployment list" -ForegroundColor Green
        Write-Host ""
        
    } catch {
        Write-Warn "Error detecting service configuration: $($_.Exception.Message)"
        Write-Log "Fallback configuration command:"
        Write-Host "$CliCommand ctx set $Environment --pm http://localhost:8080 --zenoh-peer tcp/localhost:17447" -ForegroundColor Green
    }
}

# Function to check if operators already exist
function Test-OperatorStatus {
    $Status = @{
        PrometheusOperator = $false
        KnativeOperator = $false
        PrometheusInstalled = $false
        KnativeInstalled = $false
    }
    
    # Check if Prometheus Operator CRDs exist
    try {
        kubectl get crd servicemonitors.monitoring.coreos.com 2>$null | Out-Null
        $Status.PrometheusOperator = $true
    } catch { }
    
    # Check if Knative Operator CRDs exist
    try {
        kubectl get crd knativeservings.operator.knative.dev 2>$null | Out-Null
        $Status.KnativeOperator = $true
    } catch { }
    
    # Check if Prometheus is actually installed
    try {
        kubectl get prometheus --all-namespaces 2>$null | Out-Null
        $Status.PrometheusInstalled = $true
    } catch { }
    
    # Check if Knative Serving is installed
    try {
        kubectl get knativeservings --all-namespaces 2>$null | Out-Null
        $Status.KnativeInstalled = $true
    } catch { }
    
    return $Status
}

# Function to get Helm arguments for deployment
function Get-HelmArgs {
    param(
        [string]$Component
    )
    
    $ValuesFile = ""
    $ExtraArgs = @()
    
    # Determine base values file
    switch ($Environment) {
        { $_ -in "dev", "development" } {
            $ValuesFile = "$ChartsDir\examples\$Component-development.yaml"
        }
        { $_ -in "prod", "production" } {
            $ValuesFile = "$ChartsDir\examples\$Component-production.yaml"
        }
        "staging" {
            $ValuesFile = "$ChartsDir\examples\combined-deployment.yaml"
        }
        "nodeport" {
            $ValuesFile = "$ChartsDir\examples\$Component-nodeport.yaml"
        }
    }
    
    # Add component-specific arguments
    if ($Component -eq "crm") {
        # Configure CRM to use the correct namespace
        $ExtraArgs += "--set", "config.namespace=$Namespace"
        
        # Add operator-specific arguments for CRM
        if ($Operators) {
            # Use the comprehensive operators example
            $ValuesFile = "$ChartsDir\examples\crm-with-operators.yaml"
        } else {
            # Add individual operator flags
            if ($EnablePrometheus) {
                $ExtraArgs += "--set", "prometheus.operator.enabled=true"
                $ExtraArgs += "--set", "prometheus.serviceMonitor.enabled=true"
                $ExtraArgs += "--set", "prometheus.prometheusRule.enabled=true"
            }
            
            if ($EnableKnative) {
                $ExtraArgs += "--set", "knative.operator.enabled=true"
                $ExtraArgs += "--set", "knative.operator.serving.enabled=true"
                $ExtraArgs += "--set", "knative.operator.serving.ingress.kourier.enabled=true"
                $ExtraArgs += "--set", "knative.operator.serving.dns.magic=true"
            }
        }
    } elseif ($Component -eq "pm") {
        # Configure PM to use the correct CRM service URL
        $CrmServiceName = "oaas-crm-$Environment-oprc-crm"
        $ExtraArgs += "--set", "config.crm.default.url=http://$CrmServiceName`:8088"
    }
    
    $Args = @("--values", $ValuesFile) + $ExtraArgs
    return $Args
}

# Handle help requests
if ($Help -or $Environment -eq "help" -or $Action -eq "help" -or $args -contains "--help" -or $args -contains "-h") {
    Show-Help
    exit 0
}

# Validate environment
switch ($Environment) {
    { $_ -in "dev", "development", "prod", "production", "staging", "nodeport" } {
        # Valid environment
    }
    default { Write-Error-Exit "Unknown environment: $Environment. Use dev, staging, prod, or nodeport." }
}

# Check prerequisites
try {
    $null = Get-Command helm -ErrorAction Stop
} catch {
    Write-Error-Exit "Helm is not installed. Please install Helm 3.0+ and try again."
}

try {
    $null = Get-Command kubectl -ErrorAction Stop
} catch {
    Write-Error-Exit "kubectl is not installed. Please install kubectl and try again."
}

try {
    kubectl cluster-info 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { throw }
} catch {
    Write-Error-Exit "Cannot connect to Kubernetes cluster. Please check your kubeconfig."
}

# Create namespace if it doesn't exist
try {
    kubectl get namespace $Namespace 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Log "Creating namespace: $Namespace"
        kubectl create namespace $Namespace
    }
} catch {
    Write-Log "Creating namespace: $Namespace"
    kubectl create namespace $Namespace
}

# Create knative-serving namespace if Knative is enabled
if ($EnableKnative -or $Operators) {
    try {
        kubectl get namespace knative-serving 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Log "Creating Knative namespace: knative-serving"
            kubectl create namespace knative-serving
        }
    } catch {
        Write-Log "Creating Knative namespace: knative-serving"
        kubectl create namespace knative-serving
    }
}

# Execute action
switch ($Action) {
    "install" {
        Write-Log "Installing OaaS components in $Environment environment..."
        
        # Check existing operator status
        $OperatorStatus = Test-OperatorStatus
        
        if ($Operators) {
            Write-Log "Operators enabled: Prometheus + Knative"
            if ($OperatorStatus.PrometheusInstalled) {
                Write-Log "Prometheus Operator already installed - reusing existing installation"
            }
            if ($OperatorStatus.KnativeInstalled) {
                Write-Log "Knative Operator already installed - reusing existing installation"
            }
        } elseif ($EnablePrometheus -or $EnableKnative) {
            Write-Log "Operators enabled: Prometheus=$EnablePrometheus, Knative=$EnableKnative"
            if ($EnablePrometheus -and $OperatorStatus.PrometheusInstalled) {
                Write-Log "Prometheus Operator already installed - reusing existing installation"
            }
            if ($EnableKnative -and $OperatorStatus.KnativeInstalled) {
                Write-Log "Knative Operator already installed - reusing existing installation"
            }
        }
        
        # Build chart dependencies if operators are enabled and not already installed
        $NeedOperatorSetup = ($EnablePrometheus -or $EnableKnative -or $Operators) -and 
                            ((-not $OperatorStatus.PrometheusInstalled -and ($EnablePrometheus -or $Operators)) -or 
                             (-not $OperatorStatus.KnativeInstalled -and ($EnableKnative -or $Operators)))
        
        if ($NeedOperatorSetup) {
            Write-Log "Setting up operators that are not already installed..."
            if (-not $SkipRepoUpdate) {
                helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>$null || $true
                helm repo add knative https://knative.github.io/operator 2>$null || $true
                helm repo update 2>$null || $true
            } else {
                Write-Log "Skipping helm repo update (SkipRepoUpdate flag)"
            }
            
            if (-not $SkipDependencyBuild) {
                Write-Log "Building Helm chart dependencies for operators..."
                Set-Location "$ChartsDir\oprc-crm"
                helm dependency build
                if ($LASTEXITCODE -ne 0) {
                    Write-Error-Exit "Failed to build CRM chart dependencies"
                }
                Set-Location $ChartsDir
            } else {
                Write-Log "Skipping helm dependency build for CRM (SkipDependencyBuild flag)"
            }
        } elseif ($EnablePrometheus -or $EnableKnative -or $Operators) {
            Write-Log "All requested operators already installed - skipping dependency build"
            # Still need to ensure CRM chart has dependencies for template rendering
            if (-not $SkipDependencyBuild) {
                Write-Log "Building CRM chart dependencies for template compatibility..."
                if (-not $SkipRepoUpdate) {
                    helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>$null || $true
                    helm repo add knative https://knative.github.io/operator 2>$null || $true
                    helm repo update 2>$null || $true
                } else {
                    Write-Log "Skipping helm repo update (SkipRepoUpdate flag)"
                }
                Set-Location "$ChartsDir\oprc-crm"
                helm dependency build
                Set-Location $ChartsDir
            } else {
                Write-Log "Skipping CRM dependency build for template compatibility (SkipDependencyBuild flag)"
            }
        }
        
        # Build PM chart dependencies
        Write-Log "Adding Helm repositories for PM dependencies..."
        helm repo add bitnami https://charts.bitnami.com/bitnami 2>$null || $true
        helm repo update 2>$null || $true
        
        Write-Log "Building PM chart dependencies..."
        Set-Location "$ChartsDir\oprc-pm"
        helm dependency build
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Failed to build PM chart dependencies"
        }
        Set-Location $ChartsDir
        
        # Install CRM first
        Write-Log "Installing CRM (Class Runtime Manager)..."
        $CrmArgs = Get-HelmArgs -Component "crm"
        $HelmCmd = @("install", "oaas-crm-$Environment", "$ChartsDir\oprc-crm", "--namespace", $Namespace, "--wait") + $CrmArgs
        & helm @HelmCmd
        
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Failed to install CRM"
        }
        
        # Wait for CRM to be ready
        Write-Log "Waiting for CRM to be ready..."
        kubectl wait --for=condition=available deployment/oaas-crm-$Environment-oprc-crm `
            --namespace $Namespace --timeout=300s
        
        # Install PM
        Write-Log "Installing PM (Package Manager)..."
        $PmArgs = Get-HelmArgs -Component "pm"
        $HelmCmd = @("install", "oaas-pm-$Environment", "$ChartsDir\oprc-pm", "--namespace", $Namespace, "--wait") + $PmArgs
        & helm @HelmCmd
        
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Failed to install PM"
        }
        
        Write-Log "OaaS installation completed successfully!"
        
        # Configure CLI if available
        Configure-OprcCli -Environment $Environment -Namespace $Namespace
    }
    
    "upgrade" {
        Write-Log "Upgrading OaaS components in $Environment environment..."
        
        # Check existing operator status
        $OperatorStatus = Test-OperatorStatus
        
        if ($Operators) {
            Write-Log "Operators enabled: Prometheus + Knative"
            if ($OperatorStatus.PrometheusInstalled) {
                Write-Log "Prometheus Operator already installed - reusing existing installation"
            }
            if ($OperatorStatus.KnativeInstalled) {
                Write-Log "Knative Operator already installed - reusing existing installation"
            }
        } elseif ($EnablePrometheus -or $EnableKnative) {
            Write-Log "Operators enabled: Prometheus=$EnablePrometheus, Knative=$EnableKnative"
            if ($EnablePrometheus -and $OperatorStatus.PrometheusInstalled) {
                Write-Log "Prometheus Operator already installed - reusing existing installation"
            }
            if ($EnableKnative -and $OperatorStatus.KnativeInstalled) {
                Write-Log "Knative Operator already installed - reusing existing installation"
            }
        }
        
        # Build chart dependencies if operators are enabled
        $NeedOperatorSetup = ($EnablePrometheus -or $EnableKnative -or $Operators) -and 
                            ((-not $OperatorStatus.PrometheusInstalled -and ($EnablePrometheus -or $Operators)) -or 
                             (-not $OperatorStatus.KnativeInstalled -and ($EnableKnative -or $Operators)))
        
        if ($NeedOperatorSetup) {
            Write-Log "Setting up operators that are not already installed..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>$null || $true
            helm repo add knative https://knative.github.io/operator 2>$null || $true
            helm repo update 2>$null || $true
            
            Write-Log "Building Helm chart dependencies for operators..."
            Set-Location "$ChartsDir\oprc-crm"
            helm dependency build
            if ($LASTEXITCODE -ne 0) {
                Write-Error-Exit "Failed to build CRM chart dependencies"
            }
            Set-Location $ChartsDir
        } elseif ($EnablePrometheus -or $EnableKnative -or $Operators) {
            Write-Log "All requested operators already installed - skipping dependency build"
            # Still need to ensure CRM chart has dependencies for template rendering
            Write-Log "Building CRM chart dependencies for template compatibility..."
            helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>$null || $true
            helm repo add knative https://knative.github.io/operator 2>$null || $true
            helm repo update 2>$null || $true
            Set-Location "$ChartsDir\oprc-crm"
            helm dependency build
            Set-Location $ChartsDir
        }
        
        # Build PM chart dependencies
        Write-Log "Adding Helm repositories for PM dependencies..."
        helm repo add bitnami https://charts.bitnami.com/bitnami 2>$null || $true
        helm repo update 2>$null || $true
        
        Write-Log "Building PM chart dependencies..."
        Set-Location "$ChartsDir\oprc-pm"
        helm dependency build
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Exit "Failed to build PM chart dependencies"
        }
        Set-Location $ChartsDir
        
        # Upgrade CRM
        Write-Log "Upgrading CRM..."
        $CrmArgs = Get-HelmArgs -Component "crm"
        $HelmCmd = @("upgrade", "oaas-crm-$Environment", "$ChartsDir\oprc-crm", "--namespace", $Namespace, "--wait") + $CrmArgs
        & helm @HelmCmd
        
        # Upgrade PM
        Write-Log "Upgrading PM..."
        $PmArgs = Get-HelmArgs -Component "pm"
        $HelmCmd = @("upgrade", "oaas-pm-$Environment", "$ChartsDir\oprc-pm", "--namespace", $Namespace, "--wait") + $PmArgs
        & helm @HelmCmd
        
        Write-Log "OaaS upgrade completed successfully!"
    }
    
    "uninstall" {
        Write-Warn "Uninstalling OaaS components from $Environment environment..."
        
        if ($Operators -or $EnablePrometheus -or $EnableKnative) {
            Write-Log "Operators flag detected - performing complete cleanup including operators"
        } else {
            Write-Log "Note: Operators (Prometheus, Knative) will be kept for reuse"
        }
        
        # Uninstall PM first
        Write-Log "Uninstalling PM..."
        try {
            helm uninstall "oaas-pm-$Environment" --namespace $Namespace 2>$null
        } catch {
            Write-Warn "PM release not found"
        }
        
        # Uninstall CRM
        Write-Log "Uninstalling CRM..."
        try {
            helm uninstall "oaas-crm-$Environment" --namespace $Namespace 2>$null
        } catch {
            Write-Warn "CRM release not found"
        }
        
        # Remove operators if flag is specified
        if ($Operators -or $EnablePrometheus -or $EnableKnative) {
            Write-Log "Removing operators and their components..."
            
            # Remove Knative components
            if ($EnableKnative -or $Operators) {
                Write-Log "Removing Knative Serving resources..."
                try {
                    kubectl delete knativeservings --all --all-namespaces 2>$null
                } catch {
                    Write-Warn "Some Knative Serving resources may not have been removed"
                }
                
                Write-Log "Removing Knative Operator..."
                try {
                    helm uninstall knative-operator --namespace $Namespace 2>$null
                } catch {
                    Write-Warn "Knative Operator release not found"
                }
                
                Write-Log "Cleaning up Knative CRDs..."
                try {
                    kubectl delete crd -l knative.dev/crd-install=true 2>$null
                    kubectl delete crd -l app.kubernetes.io/name=knative-serving 2>$null
                    kubectl delete crd -l app.kubernetes.io/name=knative-operator 2>$null
                } catch {
                    Write-Warn "Some Knative CRDs may not have been removed"
                }
                
                Write-Log "Removing Knative namespaces..."
                try {
                    kubectl delete namespace knative-serving --ignore-not-found=true 2>$null
                    kubectl delete namespace knative-operator --ignore-not-found=true 2>$null
                } catch {
                    Write-Warn "Some Knative namespaces may not have been removed"
                }
            }
            
            # Remove Prometheus components  
            if ($EnablePrometheus -or $Operators) {
                Write-Log "Removing Prometheus Operator..."
                try {
                    helm uninstall kube-prometheus-stack --namespace $Namespace 2>$null
                } catch {
                    Write-Warn "Prometheus Operator release not found"
                }
            }
            
            # Remove etcd (PM dependency)
            Write-Log "Removing etcd..."
            try {
                helm uninstall etcd --namespace $Namespace 2>$null
            } catch {
                Write-Warn "etcd release not found"
            }
            
            Write-Log "Complete OaaS and operators uninstallation completed!"
        } else {
            Write-Log "OaaS uninstallation completed! Operators remain available for reuse."
        }
    }
    
    "status" {
        Write-Log "Checking OaaS status in $Environment environment..."
        
        Write-Host "=== Helm Releases ===" -ForegroundColor Cyan
        helm list --namespace $Namespace
        
        Write-Host "`n=== Deployments ===" -ForegroundColor Cyan
        kubectl get deployments --namespace $Namespace
        
        Write-Host "`n=== Services ===" -ForegroundColor Cyan
        kubectl get services --namespace $Namespace
        
        Write-Host "`n=== Custom Resources ===" -ForegroundColor Cyan
        try {
            kubectl get deploymentrecords.oaas.io --namespace $Namespace 2>$null
        } catch {
            Write-Host "No DeploymentRecords found"
        }
        
        Write-Host "`n=== Operator Status ===" -ForegroundColor Cyan
        # Check for Prometheus Operator resources
        try {
            kubectl get servicemonitors.monitoring.coreos.com --namespace $Namespace 2>$null
            Write-Host "Prometheus ServiceMonitor found"
        } catch {
            Write-Host "No Prometheus ServiceMonitor"
        }
        
        try {
            kubectl get prometheusrules.monitoring.coreos.com --namespace $Namespace 2>$null
            Write-Host "Prometheus Rules found"
        } catch {
            Write-Host "No Prometheus Rules"
        }
        
        # Check for Knative resources
        try {
            kubectl get knativeservings.operator.knative.dev --all-namespaces 2>$null
            Write-Host "Knative Serving found"
        } catch {
            Write-Host "No Knative Serving"
        }
        
        Write-Host "`n=== Pods ===" -ForegroundColor Cyan
        kubectl get pods --namespace $Namespace
    }
    
    "test" {
        Write-Log "Testing OaaS deployment in $Environment environment..."
        
        # Test CRM health
        Write-Log "Testing CRM health..."
        $crmJob = Start-Job -ScriptBlock {
            kubectl port-forward "svc/oaas-crm-$using:Environment-oprc-crm" 8088:8088 --namespace $using:Namespace
        }
        Start-Sleep 5
        
        try {
            $response = Invoke-WebRequest -Uri "http://localhost:8088/health" -UseBasicParsing -TimeoutSec 10
            if ($response.StatusCode -eq 200) {
                Write-Log "CRM health check passed"
            } else {
                Write-Error-Exit "CRM health check failed"
            }
        } catch {
            Write-Error-Exit "CRM health check failed: $_"
        } finally {
            Stop-Job $crmJob -ErrorAction SilentlyContinue
            Remove-Job $crmJob -ErrorAction SilentlyContinue
        }
        
        # Test PM health
        Write-Log "Testing PM health..."
        $pmJob = Start-Job -ScriptBlock {
            kubectl port-forward "svc/oaas-pm-$using:Environment-oprc-pm" 8080:8080 --namespace $using:Namespace
        }
        Start-Sleep 5
        
        try {
            $response = Invoke-WebRequest -Uri "http://localhost:8080/health" -UseBasicParsing -TimeoutSec 10
            if ($response.StatusCode -eq 200) {
                Write-Log "PM health check passed"
            } else {
                Write-Error-Exit "PM health check failed"
            }
        } catch {
            Write-Error-Exit "PM health check failed: $_"
        } finally {
            Stop-Job $pmJob -ErrorAction SilentlyContinue
            Remove-Job $pmJob -ErrorAction SilentlyContinue
        }
        
        Write-Log "All health checks passed!"
    }
    
    "help" {
        Show-Help
    }
    
    default {
        Write-Error-Exit "Unknown action: $Action. Use install, upgrade, uninstall, status, test, or help."
    }
}

Write-Log "Action '$Action' completed for environment '$Environment'."
