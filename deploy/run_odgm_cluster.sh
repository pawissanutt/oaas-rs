#!/usr/bin/env bash

PARTITION_COUNT="${PARTITION_COUNT:-12}"
ODGM_IMAGE="${ODGM_IMAGE:-harbor.129.114.109.85.nip.io/oaas/odgm}"
MAX_SESSIONS="${MAX_SESSIONS:-1}"
ROUTER_IMAGE="${ROUTER_IMAGE:-harbor.129.114.109.85.nip.io/oaas/router}"



N=$1
if [ -z "$N" ]; then
  N=3
fi

REPLICA_COUNT="${REPLICA_COUNT:-$N}"

members=""
for (( i=1; i<=$N; i++ )); do
  if [ $i -eq 1 ]; then
    members="$i"
  else
    members="$members,$i"
  fi
done

docker run -d --name oprc-router \
  -p 7447:7447 \
  -e "OPRC_ZENOH_PORT=7447" \
  "$ROUTER_IMAGE"

for (( i=1; i<=$N; i++ )); do
  docker run -d --name odgm-$i \
    -e "ODGM_NODE_ID=$i" \
    -e "ODGM_MEMBERS=$members" \
    -e "ODGM_LOG=INFO,openraft=info,zenoh=info,h2=warn" \
    -e "ODGM_MAX_SESSIONS=$MAX_SESSIONS" \
    -e "ODGM_COLLECTION=[{\"name\":\"example\",\"partition_count\":$PARTITION_COUNT,\"replica_count\":$REPLICA_COUNT,\"shard_assignments\":[],\"shard_type\":\"raft\",\"options\":{}}]" \
    "$ODGM_IMAGE"
done