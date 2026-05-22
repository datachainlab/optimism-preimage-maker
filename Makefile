SED = $(shell which gsed 2>/dev/null || echo sed)

.PHONY: set-port
set-port:
	scripts/port.sh
	scripts/get_l1_config.sh

.PHONY: set-port-fixed
set-port-fixed:
	echo "{\"l1BeaconPort\": 9596, \"l1GethPort\": 8545, \"l2RollupPort\": 9545, \"l2GethPort\": 8546}" | jq > hostPort.json

.PHONY: status
status:
	@PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	curl -X POST localhost:$$PORT -d '{"method":"optimism_syncStatus", "jsonrpc": "2.0", "id":1, "params":[]}' -H "Content-Type: application/json" | jq .result

.PHONY: wait
wait:
	./scripts/wait.sh

.PHONY: server-up
server-up:
	$(MAKE) _server-up PREIMAGE_DISTANCE=50

.PHONY: server-up-long
server-up-long:
	$(MAKE) _server-up PREIMAGE_DISTANCE=300

.PHONY: _server-up
_server-up:
	mkdir -p .preimage && true
	mkdir -p .finalized_l1 && true
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L1_GETH_PORT=$$(jq -r '.l1GethPort' hostPort.json);\
	L1_BEACON_PORT=$$(jq -r '.l1BeaconPort' hostPort.json);\
	L1_CHAIN_CONFIG=$$(cat l1_chain_config.json | base64 -w 0);\
	cargo run --release --features=minimal --bin=optimism-preimage-maker -- \
		--rollup=http://localhost:$$L2_ROLLUP_PORT \
		--l2=http://localhost:$$L2_GETH_PORT \
		--l1=http://localhost:$$L1_GETH_PORT \
		--beacon=http://localhost:$$L1_BEACON_PORT \
		--l1-chain-config=$$L1_CHAIN_CONFIG \
		--initial-claimed-l2=53 \
		--ttl=1800 \
		--preimage-distance=$(PREIMAGE_DISTANCE) \
		--purger-interval-seconds=100

.PHONY: test
test:
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L2_ROLLUP_ADDR=http://localhost:$$L2_ROLLUP_PORT L2_GETH_ADDR=http://localhost:$$L2_GETH_PORT cargo test --features=minimal --manifest-path=./server/Cargo.toml

.PHONY: inspect
inspect:
	cargo test --manifest-path=./server/Cargo.toml -- --ignored

.PHONY: sync-lock
sync-lock:
	cargo update -p kona-client
	cd scripts && python sync_lock.py
	# Check build
	# Downgrade the crate that does not exist in op-rs, which was unnecessarily upgraded by cargo update.
	cargo build

.PHONY: test-deploy-tx
test-deploy-tx:
	cd tx && npm install && npx hardhat run ./scripts/deploy.js --network eth_local

.PHONY: test-tx
test-tx:
	cd tx && npx hardhat run ./scripts/exec.js --network eth_local
