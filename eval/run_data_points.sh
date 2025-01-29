#!/usr/bin/env bash
trap "exit" INT

ITERATION_COUNT="${ITERATION_COUNT:-10}"
EXAMPLE="${EXAMPLE:-10}"
CONCURRENCY="${CONCURRENCY:-100}"
REQUEST_RATE="${REQUEST_RATE:-1000}"
REQUEST_SCALE="${REQUEST_SCALE:-2}"

ITERATION_COUNT=$1
shift

if [[ -e "data_points.ndjson" ]]; then
  > data_points.ndjson
fi

while getopts "e:c:r:s:" opt; do
  case "$opt" in
    e) EXAMPLE="${OPTARG}";;
    c) CONCURRENCY="${OPTARG}";;
    r) REQUEST_RATE="${OPTARG}";;
    s) REQUEST_SCALE="${OPTARG}";;
    *) echo "wrong flags"; exit 1;;
  esac
done 

for (( i=1; i<="$ITERATION_COUNT"; i++ )); do
        result=$(bench-kv-set example "$EXAMPLE" -d 10s -c "$CONCURRENCY" -r "$REQUEST_RATE" -t 8 -o json)
        echo "$result" | jq --argjson result "$result" --argjson rate "$REQUEST_RATE" \
	'{"result": $result, "request_rate": $rate}' | jq -c >> data_points.ndjson
	REQUEST_RATE=$((REQUEST_RATE + REQUEST_SCALE))
	echo "$i done"
done
 
cat data_points.ndjson


