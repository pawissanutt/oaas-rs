# Knative minimal Serving deployment helper (no Eventing)
# Usage:
#   ./deploy-knative.ps1 install          # Install/ensure operator + serving
#   ./deploy-knative.ps1 uninstall        # Remove Knative Serving + operator (optional)
#   ./deploy-knative.ps1 status           # Show Knative components status
# Flags:
#   -Namespace <ns>   Namespace to install operator (default: knative-operator)
#   -Domain <domain>  Domain entry for config-domain (default: example.com)
#   -SkipRepo         Skip helm repo add/update (assumes cached)
param(
  [Parameter(Position=0)] [ValidateSet('install','uninstall','status','help')] [string]$Action='install',
  [string]$Namespace='knative-operator',
  [string]$Domain='example.com',
  [switch]$SkipRepo,
  [switch]$ForceRecreate,   # Force delete existing Knative CRDs instead of attempting adoption
  [switch]$HelmDebug,       # Pass --debug and show full helm output on failure
  [switch]$Help
)

if ($Help -or $Action -eq 'help') {
  Write-Host 'Knative Serving deploy helper' -ForegroundColor Cyan
  Write-Host 'Examples:'
  Write-Host '  ./deploy-knative.ps1 install -Namespace oaas-dev' -ForegroundColor Green
  Write-Host '  ./deploy-knative.ps1 uninstall -Namespace oaas-dev' -ForegroundColor Green
  Write-Host '  ./deploy-knative.ps1 install -ForceRecreate   # Purge and recreate CRDs instead of adopting' -ForegroundColor Green
  exit 0
}

function Log($m){ Write-Host "[INFO] $m" -ForegroundColor Green }
function Warn($m){ Write-Host "[WARN] $m" -ForegroundColor Yellow }
function Err($m){ Write-Host "[ERROR] $m" -ForegroundColor Red; exit 1 }

# Ensure pre-existing Knative Operator CRDs (possibly created by earlier embedded chart) don't block Helm adoption
function Set-KnativeCRDOwnership {
  $crds = @('knativeservings.operator.knative.dev','knativeeventings.operator.knative.dev')
  foreach ($c in $crds) {
    $json = kubectl get crd $c -o json 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $json) { continue }
    try { $obj = $json | ConvertFrom-Json } catch { continue }
    if (-not $obj.metadata.annotations) { $obj.metadata | Add-Member -NotePropertyName annotations -NotePropertyValue @{} }
    $rn = $obj.metadata.annotations.'meta.helm.sh/release-name'
    $rns = $obj.metadata.annotations.'meta.helm.sh/release-namespace'
    if ($rn -eq 'knative-operator' -and $rns -eq $Namespace) { continue }
  Log "Patching existing CRD $c to set Helm ownership to knative-operator/$Namespace (was ${rn}/${rns})"
  $patch = '{"metadata":{"annotations":{"meta.helm.sh/release-name":"knative-operator","meta.helm.sh/release-namespace":"'+$Namespace+'","meta.helm.sh/release-version":"0","app.kubernetes.io/managed-by":"Helm"}}}'
    kubectl patch crd $c --type merge -p $patch 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { Warn "Failed to patch CRD $c annotations; Helm install may still fail" }
  }
}

switch ($Action) {
  'status' {
    Log 'Collecting Knative status...'
    kubectl get pods -n knative-serving 2>$null
    kubectl get knativeserving -A 2>$null
    exit 0
  }
  'uninstall' {
    Log 'Removing Knative Serving CR...'
    kubectl delete knativeserving knative-serving -n knative-serving --ignore-not-found 2>$null
    Log 'Removing namespace knative-serving (will fail if other apps use it)'
    kubectl delete namespace knative-serving --ignore-not-found 2>$null
    Log 'Uninstalling Knative Operator Helm release (if in same namespace)'
    helm uninstall knative-operator -n $Namespace 2>$null
    exit 0
  }
  'install' {
    if (-not $SkipRepo) {
      Log 'Adding/updating Helm repo knative'
      helm repo add knative https://knative.github.io/operator 2>$null || $true
      helm repo update 2>$null || $true
    } else { Log 'Skipping repo update' }

    if ($ForceRecreate) {
      Log 'ForceRecreate enabled: backing up and deleting existing Knative CRDs (if any)'
      $crdsToHandle = @('knativeservings.operator.knative.dev','knativeeventings.operator.knative.dev')
      foreach ($c in $crdsToHandle) {
        kubectl get crd $c 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
          $backup = Join-Path $env:TEMP "$c-backup.yaml"
          kubectl get crd $c -o yaml > $backup 2>$null
          Log "Deleting CRD $c (backup: $backup)"
          kubectl delete crd $c 2>$null | Out-Null
        }
      }
    } else {
      Log 'Reconciling existing Knative CRD ownership (if any)'
      Set-KnativeCRDOwnership
    }

    function Invoke-HelmInstall {
      $helmArgs = @('upgrade','--install','knative-operator','knative/knative-operator','--namespace', $Namespace,'--create-namespace','--wait')
      if ($HelmDebug) { $helmArgs += '--debug' }
      if ($HelmDebug) { Log "Helm command: helm $($helmArgs -join ' ')" }
      $out = helm @helmArgs 2>&1
      $code = $LASTEXITCODE
      if ($code -ne 0 -and $HelmDebug) {
        Write-Host '----- Helm Error Output Start -----' -ForegroundColor Red
        $out | Out-Host
        Write-Host '----- Helm Error Output End -----' -ForegroundColor Red
      }
      return $code
    }

    Log 'Installing / updating Knative Operator (attempt 1)'
    $code = Invoke-HelmInstall
    if ($code -ne 0 -and -not $ForceRecreate) {
      Warn 'Initial Helm install failed. Attempting CRD purge + retry (non-destructive to data)'
      $crdsToHandle = @('knativeservings.operator.knative.dev','knativeeventings.operator.knative.dev')
      foreach ($c in $crdsToHandle) {
        kubectl get crd $c 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
          $backup = Join-Path $env:TEMP "$c-backup.yaml"
            kubectl get crd $c -o yaml > $backup 2>$null
            Log "Deleting CRD $c before retry (backup: $backup)"
            kubectl delete crd $c 2>$null | Out-Null
        }
      }
      Log 'Retrying Helm install (attempt 2)'
      $code = Invoke-HelmInstall
    }
    if ($code -ne 0) {
      if (-not $HelmDebug) { Warn 'Re-run with -HelmDebug for detailed Helm diagnostics' }
      Err 'Failed to install knative-operator'
    }

    Log 'Ensuring knative-serving namespace'
    kubectl get ns knative-serving 2>$null || kubectl create namespace knative-serving | Out-Null

    Log 'Applying KnativeServing CR'
@"
apiVersion: operator.knative.dev/v1beta1
kind: KnativeServing
metadata:
  name: knative-serving
  namespace: knative-serving
spec:
  config:
    domain:
      $($Domain): ""
    network:
      ingress.class: kourier.ingress.networking.knative.dev
  ingress:
    kourier:
      enabled: true
"@ | kubectl apply -f - | Out-Null

    Log 'Waiting for webhook service (up to 120s)'
    $ok=$false; for($i=0;$i -lt 60;$i++){ kubectl get svc webhook -n knative-serving 2>$null | Out-Null; if($LASTEXITCODE -eq 0){$ok=$true;break}; Start-Sleep 2 }
    if($ok){ Log 'Webhook service ready' } else { Warn 'Webhook service not ready yet' }

    Log 'Knative Serving install sequence complete'
  }
}
