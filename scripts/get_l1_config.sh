#!/bin/bash

container=`docker ps | grep "el-1-" | awk '{print $NF}'`
docker exec ${container} cat /network-configs/genesis.json | jq .config > l1_chain_config.json
