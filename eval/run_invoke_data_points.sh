#!/usr/bin/env bash
trap "exit" INT

ITERATION_COUNT="${ITERATION_COUNT:-10}"
PARTITIONS="${PARTITIONS:-12}"
CONCURRENCY="${CONCURRENCY:-100}"
REQUEST_RATE="${REQUEST_RATE:-100}"
REQUEST_SCALE="${REQUEST_SCALE:-100}"
LOG_FILE="${LOG_FILE:-output/invocations.ndjson}"
GROUP="${GROUP:-default}"

ITERATION_COUNT=${1:-10}
shift

# if [[ -e "data_points.ndjson" ]]; then
#   > data_points.ndjson
# fi

while getopts "p:c:r:s:g:" opt; do
  case "$opt" in
    p) PARTITIONS="${OPTARG}";;
    c) CONCURRENCY="${OPTARG}";;
    r) REQUEST_RATE="${OPTARG}";;
    s) REQUEST_SCALE="${OPTARG}";;
    g) GROUP="${OPTARG}";;
    *) echo "wrong flags"; exit 1;;
  esac
done 

mkdir -p "$(dirname "$LOG_FILE")"

echo "REQUEST_RATE: $REQUEST_RATE"
echo "REQUEST_SCALE: $REQUEST_SCALE"
echo "GROUP: $GROUP"
for (( i=1; i<="$ITERATION_COUNT"; i++ )); do
  RESULT=$(bench-z-invoke  example.record echo "$PARTITIONS" \
          -d 10s \
          -z "tcp/localhost:17447" \
          -c "$CONCURRENCY" \
          -r "$REQUEST_RATE" \
          -t 8 \
          --peer \
          -q -o json)
  # echo "round $i: $(echo $RESULT | jq -c .)"
  # echo "REQUEST_SCALE: $REv
  RESULT=$(echo "$RESULT" | jq -c \
    --argjson rate "$REQUEST_RATE"  \
    --argjson group \"$GROUP\"  \
    '{result: ., request_rate: $rate, group: $group}') 
  echo $RESULT >> $LOG_FILE
  REQUEST_RATE=$((REQUEST_RATE + REQUEST_SCALE))
  echo "round $i: $RESULT"
done
 
cat $LOG_FILE


