cri := "docker"
# cri := "podman"

build-release:
  {{cri}} compose -f docker-compose.release.yml build gateway
  {{cri}} compose -f docker-compose.release.yml build 

compose-release: build-release
  {{cri}} compose -f docker-compose.release.yml up -d

dev-up flag="": 
  cargo build {{flag}}
  {{cri}} compose -f docker-compose.dev.yml up -d



push-release: build-release
  {{cri}} compose -f docker-compose.release.yml push
  # {{cri}} push ghcr.io/pawissanutt/oaas/gateway
  # {{cri}} push ghcr.io/pawissanutt/oaas/odgm
  # {{cri}} push ghcr.io/pawissanutt/oaas/echo-fn
  # {{cri}} push ghcr.io/pawissanutt/oaas/dev-pm



k8s-deploy-dev: k8s-deploy-deps
  kubectl apply -n oaas -k deploy/oaas/dev
  kubectl apply -n oaas -f deploy/cc/oprc-ingress.yml
  kubectl apply -n oaas -f deploy/local-k8s/oprc-np.yml


k8s-deploy-deps:
  kubectl apply -n oaas -f deploy/local-k8s/kafka-cluster.yml
  kubectl apply -n oaas -f deploy/local-k8s/minio.yml
  kubectl apply -n oaas -f deploy/cc/minio-ingress.yml
  kubectl apply -n oaas -f deploy/local-k8s/arango-single.yml
  kubectl apply -n oaas -f deploy/cc/arango-ingress.yml

k8s-clean:
  kubectl delete -n oaas ksvc -l oaas.function || true
  kubectl delete -n oaas -f deploy/local-k8s/arango-single.yml || true
  kubectl delete -n oaas -f deploy/local-k8s/arango-ingress.yml || true
  kubectl delete -n oaas -f deploy/local-k8s/minio.yml || true
  kubectl delete -n oaas -f deploy/cc/minio-ingress.yml || true
  kubectl delete -n oaas -f deploy/local-k8s/kafka-ui.yml || true
  kubectl delete -n oaas -f deploy/local-k8s/kafka-cluster.yml || true
  kubectl delete -n oaas -f deploy/cc/oprc-ingress.yml || true
  kubectl delete -n oaas -f deploy/cc/hash-aware-lb.yml || true
  kubectl delete -n oaas -k deploy/oaas/base || true

k8s-reload:
  kubectl -n oaas rollout restart deployment -l platform=oaas