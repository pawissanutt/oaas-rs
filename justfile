
build-release:
    podman compose -f docker-compose.release.yml build odgm
    podman compose -f docker-compose.release.yml build 

push-release: build-release
    podman compose -f docker-compose.release.yml push