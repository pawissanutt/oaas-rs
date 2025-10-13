set export
set dotenv-load := true

build BUILD_PROFILE="release":
  $CRI compose -f docker-compose.build.yml build pm
  $CRI compose -f docker-compose.build.yml build

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

install-tools:
  cargo install --path tools/oprc-cli
  cargo install --path tools/oprc-util-tools --features all
  cargo install --path data-plane/oprc-dev --bin check-delay

cloc:
  cloc . --exclude-dir=target

deploy:
  ./k8s/charts/deploy.sh deploy 

undeploy:
  ./k8s/charts/deploy.sh undeploy