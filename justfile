cri := "docker"
# cri := "podman"
set export

build-release cri="docker":
  {{cri}} compose -f docker-compose.release.yml build gateway
  {{cri}} compose -f docker-compose.release.yml build 


compose-dev:
  {{cri}} compose up -d

compose-release: build-release
  {{cri}} compose -f docker-compose.release.yml up -d

dev-up flag="": 
  cargo build {{flag}}
  {{cri}} compose -f docker-compose.dev.yml up -d

push-release cri="docker":
  @just build-release {{cri}}
  {{cri}} compose -f docker-compose.release.yml push

push-release-git cri="podman" IMAGE_PREFIX="ghcr.io/pawissanutt/oaas": 
  @just build-release {{cri}}
  {{cri}} push {{IMAGE_PREFIX}}/gateway
  {{cri}} push {{IMAGE_PREFIX}}/odgm
  {{cri}} push {{IMAGE_PREFIX}}/echo-fn
  {{cri}} push {{IMAGE_PREFIX}}/random-fn
  {{cri}} push {{IMAGE_PREFIX}}/router
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


install-tools:
  cargo install --path tools/oprc-cli
  # cargo install --path oprc-odgm
  # cargo install --path oprc-router
  cargo install --path tools/oprc-util-tools
  cargo install --path data-plane/oprc-dev --bin check-delay

chmod-scripts:
  chmod +x ./deploy/*.sh


check-status end="6" start="0" router="tcp/localhost:7447" collection="example.record":
  #!/usr/bin/env bash
  for (( i=$start; i<$end; i++ )); do oprc-cli o s $collection $i 0 -z $router --peer || true; done
  echo "-------------------"
  for (( i=$start; i<$end; i++ )); do oprc-cli i $collection $i random -o 0 -z $router || true; done
  echo "-------------------"
  for (( i=$start; i<$end; i++ )); do echo ping-$i |oprc-cli i $collection $i echo -z $router -p - || true; done
  
