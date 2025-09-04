set export
set dotenv-load := true

build-release BUILD_PROFILE="release":
  $CRI compose -f docker-compose.release.yml build pm
  $CRI compose -f docker-compose.release.yml build

compose-dev:
  $CRI compose up -d

compose-release: build-release
  $CRI compose -f docker-compose.release.yml up -d

push-release:
  @just build-release
  $CRI compose -f docker-compose.release.yml push

push-debug BUILD_PROFILE="debug":
  @just build-release {{BUILD_PROFILE}}
  $CRI compose -f docker-compose.release.yml push


push-release-git: 
  @just build-release
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

check-status end="6" start="0" router="tcp/localhost:7447" collection="example.record":
  #!/usr/bin/env bash
  for (( i=$start; i<$end; i++ )); do oprc-cli o s $collection $i 0 -z $router --peer || true; done
  echo "-------------------"
  for (( i=$start; i<$end; i++ )); do oprc-cli i $collection $i random -o 0 -z $router || true; done
  echo "-------------------"
  for (( i=$start; i<$end; i++ )); do echo ping-$i |oprc-cli i $collection $i echo -z $router -p - || true; done
  

cloc:
  cloc . --exclude-dir=target