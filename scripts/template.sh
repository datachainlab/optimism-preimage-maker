#!/bin/bash

L2_ROLLUP_PORT=$1
L2_GETH_PORT=$2
L1_BEACON_PORT=$3
L1_GETH_PORT=$4

mkdir -p cache/nginx/sites-enabled

sed \
    -e "s/L1_BEACON_PORT/${L1_BEACON_PORT}/g" \
    cache/nginx/templates/cache-proxy-beacon.tmpl > cache/nginx/sites-enabled/cache-proxy-beacon

sed \
    -e "s/L2_GETH_PORT/${L2_GETH_PORT}/g" \
    cache/nginx/templates/cache-proxy-op-geth.tmpl > cache/nginx/sites-enabled/cache-proxy-op-geth

sed \
    -e "s/L1_GETH_PORT/${L1_GETH_PORT}/g" \
    cache/nginx/templates/cache-proxy-geth.tmpl > cache/nginx/sites-enabled/cache-proxy-geth
