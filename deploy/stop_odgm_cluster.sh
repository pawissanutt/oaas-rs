#!/usr/bin/env bash

docker ps -q --filter name=^/oprc-router | xargs -r -I {} -P 4 docker stop {}
docker ps -aq --filter name=^/oprc-router | xargs -r -I {} -P 4 docker rm {}

docker ps -q --filter name=^/odgm- | xargs -r -I {} -P 10 docker stop {}
docker ps -aq --filter name=^/odgm- | xargs -r -I {} -P 10 docker rm {}