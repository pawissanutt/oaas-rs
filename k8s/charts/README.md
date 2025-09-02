# OaaS Kubernetes Charts

Simple Helm charts and helpers to deploy OaaS components on Kubernetes.

## What’s here
- Charts: `oprc-pm/` (Package Manager), `oprc-crm/` (Class Runtime Manager)
- Examples: `examples/pm.yaml`, `examples/crm-1.yaml`, `examples/crm-2.yaml`
- Scripts: `deploy.sh` (single-profile deploy), `deploy-knative.sh` (Knative Serving helper)

## Topology (default)
```mermaid
flowchart LR
  subgraph "Namespace: oaas"
    PM["PM (oprc-pm)"]
  end
  subgraph "Namespace: oaas-1"
    CRM1["CRM #1 (oprc-crm)"]
  end
  subgraph "Namespace: oaas-2"
    CRM2["CRM #2 (oprc-crm)"]
  end

  PM -- gRPC --> CRM1
  PM -- gRPC --> CRM2
```

## Quick start
- Deploy (PM + 2 CRMs):
  - `./deploy.sh deploy`
- Status:
  - `./deploy.sh status`
- Undeploy (keep namespaces):
  - `./deploy.sh undeploy`
- Undeploy and delete namespaces:
  - `./deploy.sh undeploy --purge-namespaces`

Notes
- Per‑CRM values live under `examples/crm-N.yaml` (N = 1..).
- PM values are in `examples/pm.yaml`; the script sets the default CRM URL automatically.
