#!/bin/bash

PORT=$(jq -r '.l2RollupPort' hostPort.json)

for i in {1..150}
do
  L2_NUMBER=$(curl -X POST localhost:$PORT -d '{"method":"optimism_syncStatus", "jsonrpc": "2.0", "id":1, "params":[]}' -H "Content-Type: application/json" | jq .result.finalized_l2.number)
  echo $L2_NUMBER
  if [ $L2_NUMBER -gt 200 ]; then
    exit 0
  fi
  echo "waiting for L2 number to be greater than 100"
  sleep 10
done
exit 1