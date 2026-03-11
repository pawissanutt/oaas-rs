set export := true
set dotenv-load := true

cri_cmd := if `command -v podman >/dev/null 2>&1 && echo found || echo notfound` == "found" { "podman" } else { "docker" }


build-cargo:
    cargo build -r
    cargo build -p wasm-guest-echo --target wasm32-wasip2

build BUILD_PROFILE="release":
    {{ cri_cmd }} compose -f docker-compose.build.yml build crm
    {{ cri_cmd }} compose -f docker-compose.build.yml build

build-pm-gui BUILD_PROFILE="release":
    {{ cri_cmd }} compose -f docker-compose.build.yml build pm

push-pm-gui BUILD_PROFILE="release":
    @just build-pm-gui {{ BUILD_PROFILE }}
    {{ cri_cmd }} compose -f docker-compose.build.yml push pm

compose-dev:
    {{ cri_cmd }} compose up -d

compose-release: build
    {{ cri_cmd }} compose -f docker-compose.build.yml up -d

push BUILD_PROFILE="debug":
    @just build {{ BUILD_PROFILE }}
    {{ cri_cmd }} compose -f docker-compose.build.yml push

push-release-git:
    @just build release
    {{ cri_cmd }} push $IMAGE_PREFIX/gateway
    {{ cri_cmd }} push $IMAGE_PREFIX/odgm
    {{ cri_cmd }} push $IMAGE_PREFIX/echo-fn
    {{ cri_cmd }} push $IMAGE_PREFIX/random-fn
    {{ cri_cmd }} push $IMAGE_PREFIX/router
    {{ cri_cmd }} push $IMAGE_PREFIX/crm
    {{ cri_cmd }} push $IMAGE_PREFIX/pm

push-local BUILD_PROFILE="debug":
    IMAGE_PREFIX="localhost:5000" just build {{ BUILD_PROFILE }}
    {{ cri_cmd }} push localhost:5000/gateway:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/odgm:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/router:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/crm:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/pm:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/echo-fn:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/random-fn:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/num-log-fn:${IMAGE_VERSION}
    {{ cri_cmd }} push localhost:5000/event-logger-fn:${IMAGE_VERSION}

system-e2e:
    cargo run -p system-e2e

system-e2e-clean:
    kind delete cluster --name ${OAAS_E2E_CLUSTER_NAME:-oaas-e2e}

install-tools:
    cargo install --path tools/oprc-cli
    # cargo install --path data-plane/oprc-dev --bin check-delay

cloc:
    cloc . --exclude-dir=target,node_modules,.next

build-compiler:
    {{ cri_cmd }} build -f tools/oprc-compiler/Dockerfile -t ${IMAGE_PREFIX:-oprc}/compiler:${IMAGE_VERSION:-latest} .

push-compiler: build-compiler
    {{ cri_cmd }} push ${IMAGE_PREFIX:-oprc}/compiler:${IMAGE_VERSION:-latest}

build-wasm-guest:
    cargo build -p wasm-guest-echo --target wasm32-wasip2 --release
    cd tools/oprc-compiler && npx tsx scripts/build-bench-guest.ts

# Defaults from environment or hardcoded fallback

REGISTRY_DEFAULT := env('IMAGE_PREFIX', 'ghcr.io/pawissanutt/oaas-rs')
TAG_DEFAULT := env('IMAGE_VERSION', 'latest')

deploy REGISTRY=REGISTRY_DEFAULT TAG=TAG_DEFAULT:
    ./k8s/charts/deploy.sh deploy --registry {{ REGISTRY }} --tag {{ TAG }}

deploy-no-compiler REGISTRY=REGISTRY_DEFAULT TAG=TAG_DEFAULT:
    ./k8s/charts/deploy.sh deploy --registry {{ REGISTRY }} --tag {{ TAG }} --no-compiler

update REGISTRY=REGISTRY_DEFAULT TAG=TAG_DEFAULT BUILD_PROFILE="debug":
    @just undeploy
    IMAGE_PREFIX={{ REGISTRY }} IMAGE_VERSION={{ TAG }} just push {{ BUILD_PROFILE }}
    @just deploy {{ REGISTRY }} {{ TAG }}

undeploy:
    ./k8s/charts/deploy.sh undeploy

create-cluster NAME="oaas-e2e":
    ./tools/kind-with-registry.sh {{ NAME }}
