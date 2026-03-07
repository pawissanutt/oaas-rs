#!/bin/sh
set -o errexit

CLUSTER_NAME="${1:-kind}"
REG_NAME='kind-registry'
REG_PORT='5001'

# Auto-detect container CLI: prefer docker (only if functional), fall back to podman
if docker --version >/dev/null 2>&1; then
  DOCKER=docker
elif podman --version >/dev/null 2>&1; then
  DOCKER=podman
  export KIND_EXPERIMENTAL_PROVIDER=podman
else
  echo "Error: neither docker nor podman found in PATH" >&2
  exit 1
fi
echo "Using container CLI: ${DOCKER}"

# 1. Create registry container unless it already exists
if [ "$(${DOCKER} inspect -f '{{.State.Running}}' "${REG_NAME}" 2>/dev/null || true)" != 'true' ]; then
  ${DOCKER} run \
    -d --restart=always -p "127.0.0.1:${REG_PORT}:5000" --name "${REG_NAME}" \
    docker.io/registry:2
fi

# 2. Create kind cluster with containerd registry config dir enabled
if ! kind get clusters 2>&1 | grep -q "^${CLUSTER_NAME}$"; then
  cat <<EOF | kind create cluster --name "${CLUSTER_NAME}" --config=-
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  extraPortMappings:
  - containerPort: 30180
    hostPort: 30180
    protocol: TCP
  - containerPort: 30081
    hostPort: 30081
    protocol: TCP
containerdConfigPatches:
- |-
  [plugins."io.containerd.grpc.v1.cri".registry]
    config_path = "/etc/containerd/certs.d"
EOF
fi

# 3. Add the registry config to the nodes
REGISTRY_DIR="/etc/containerd/certs.d/localhost:${REG_PORT}"
for node in $(kind get nodes --name "${CLUSTER_NAME}"); do
  ${DOCKER} exec "${node}" mkdir -p "${REGISTRY_DIR}"
  cat <<EOF | ${DOCKER} exec -i "${node}" cp /dev/stdin "${REGISTRY_DIR}/hosts.toml"
[host."http://${REG_NAME}:5000"]
EOF
done

# 4. Connect the registry to the cluster network if not already connected
if [ "$(${DOCKER} inspect -f='{{json .NetworkSettings.Networks.kind}}' "${REG_NAME}")" = 'null' ]; then
  ${DOCKER} network connect "kind" "${REG_NAME}"
fi

# 5. Document the local registry
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: ConfigMap
metadata:
  name: local-registry-hosting
  namespace: kube-public
data:
  localRegistryHosting.v1: |
    host: "localhost:${REG_PORT}"
    help: "https://kind.sigs.k8s.io/docs/user/local-registry/"
EOF
