#!/usr/bin/env bash

PARTITION_COUNT="${PARTITION_COUNT:-12}"
MAX_SESSIONS="${MAX_SESSIONS:-4}"
ODGM_BIN="${ODGM_BIN:-oprc-odgm}"
ROUTER_BIN="${ROUTER_BIN:-oprc-router}"

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

echo "Starting router..."
OPRC_ZENOH_PORT=7447 "$ROUTER_BIN" &

for (( i=1; i<=$N; i++ )); do
  echo "Starting ODGM node $i..."
  ODGM_NODE_ID=$i \
  ODGM_MEMBERS=$members \
  ODGM_LOG="INFO,openraft=info,zenoh=info,h2=warn" \
  ODGM_MAX_SESSIONS=$MAX_SESSIONS \
  ODGM_COLLECTION="[{\"name\":\"example\",\"partition_count\":$PARTITION_COUNT,\"replica_count\":$REPLICA_COUNT,\"shard_assignments\":[],\"shard_type\":\"raft\",\"options\":{}}]" \
  "$ODGM_BIN" &
done

wait