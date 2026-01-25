#!/bin/bash

function hostPort() {
  prefix=$1
  port=$2
  echo "Getting host port for ${prefix}:${port}"
  result=`docker ps | grep ${prefix} | awk '{print $NF}' | xargs docker inspect | jq '.[].NetworkSettings.Ports' | jq ".\"${port}/tcp\"" | jq -r '.[0].HostPort'`
  eval "$3=${result}"
}

hostPort "cl-1-lodestar-geth" 4000 l1BeaconPort
hostPort "el-1-geth-lodestar" 8545 l1GethPort
hostPort "op-cl-" 8547 l2RollupPort
hostPort "op-el-" 8545 l2GethPort

echo "{\"l1BeaconPort\": ${l1BeaconPort}, \"l1GethPort\": ${l1GethPort}, \"l2RollupPort\": ${l2RollupPort}, \"l2GethPort\": ${l2GethPort}}" | jq > hostPort.json
sed "s/L2_GETH_PORT/${l2GethPort}/g" "$(dirname "$0")/../tx/hardhat.config.template" > "$(dirname "$0")/../tx/hardhat.config.js"

cat hostPort.json