set export
set dotenv-load := true

build BUILD_PROFILE="release":
  $CRI compose -f docker-compose.build.yml build pm
  $CRI compose -f docker-compose.build.yml build

build-pm-gui BUILD_PROFILE="release":
  $CRI compose -f docker-compose.build.yml build pm
  
push-pm-gui BUILD_PROFILE="release":
  @just build-pm-gui {{BUILD_PROFILE}}
  $CRI compose -f docker-compose.build.yml push pm

compose-dev:
  $CRI compose up -d

compose-release: build
  $CRI compose -f docker-compose.build.yml up -d

push BUILD_PROFILE="debug":
  @just build {{BUILD_PROFILE}}
  $CRI compose -f docker-compose.build.yml push

push-release-git: 
  @just build release
  $CRI push $IMAGE_PREFIX/gateway
  $CRI push $IMAGE_PREFIX/odgm
  $CRI push $IMAGE_PREFIX/echo-fn
  $CRI push $IMAGE_PREFIX/random-fn
  $CRI push $IMAGE_PREFIX/router
  $CRI push $IMAGE_PREFIX/crm
  $CRI push $IMAGE_PREFIX/pm

system-e2e:
  cargo run -p system-e2e

system-e2e-clean:
  kind delete cluster --name ${OAAS_E2E_CLUSTER_NAME:-oaas-e2e}

install-tools:
  cargo install --path tools/oprc-cli
  # cargo install --path data-plane/oprc-dev --bin check-delay

cloc:
  cloc . --exclude-dir=target

deploy REGISTRY="ghcr.io/pawissanutt/oaas-rs" TAG="latest":
  ./k8s/charts/deploy.sh deploy --registry {{REGISTRY}} --tag {{TAG}}

update REGISTRY="ghcr.io/pawissanutt/oaas-rs" TAG="latest":
  @just undeploy
  IMAGE_PREFIX={{REGISTRY}} IMAGE_VERSION={{TAG}} just push debug
  @just deploy {{REGISTRY}} {{TAG}}

undeploy:
  ./k8s/charts/deploy.sh undeploy

create-cluster NAME="oaas-e2e":
  ./tools/kind-with-registry.sh {{NAME}}