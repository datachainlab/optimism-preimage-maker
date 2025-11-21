SED = $(shell which gsed 2>/dev/null || echo sed)

.PHONY: chain
chain:
	git clone --depth 1 -b v1.16.2 https://github.com/ethereum-optimism/optimism ./chain
	# override devnet config
	cp kurtosis/kurtosis.yaml ./chain/kurtosis-devnet/optimism-package-trampoline/kurtosis.yml
	cp kurtosis/main.star ./chain/kurtosis-devnet/optimism-package-trampoline/main.star
	cp kurtosis/simple.yaml ./chain/kurtosis-devnet/simple.yaml
	# devnet L1ChainConfig
	cp kurtosis/op-service/eth/config.go ./chain/op-service/eth/config.go
	# kurtosis 1.11.1 is required
	$(SED) -i 's/v1.8.2-0.20250602144112-2b7d06430e48/v1.11.1/g' ./chain/go.mod
	cd chain && go mod tidy

.PHONY: devnet-up
devnet-up:
	cd chain/kurtosis-devnet && just simple-devnet

.PHONY: set-port
set-port:
	scripts/port.sh
	scripts/get_l1_config.sh

.PHONY: status
status:
	@PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	curl -X POST localhost:$$PORT -d '{"method":"optimism_syncStatus", "jsonrpc": "2.0", "id":1, "params":[]}' -H "Content-Type: application/json" | jq .result

.PHONY: wait
wait:
	./scripts/wait.sh

.PHONY: server-up
server-up:
	mkdir -p .preimage && true
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L1_GETH_PORT=$$(jq -r '.l1GethPort' hostPort.json);\
	L1_BEACON_PORT=$$(jq -r '.l1BeaconPort' hostPort.json);\
	L1_CHAIN_CONFIG=$$(cat l1_chain_config.json | base64 -w 0);\
	cargo run --release --bin=optimism-preimage-maker -- \
		--rollup=http://localhost:$$L2_ROLLUP_PORT \
		--l2=http://localhost:$$L2_GETH_PORT \
		--l1=http://localhost:$$L1_GETH_PORT \
		--beacon=http://localhost:$$L1_BEACON_PORT \
		--l1-chain-config=$$L1_CHAIN_CONFIG

.PHONY: test
test:
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L2_ROLLUP_PORT=$$L2_ROLLUP_PORT L2_GETH_PORT=$$L2_GETH_PORT cargo test --manifest-path=./server/Cargo.toml

.PHONY: devnet-down
devnet-down:
	@ENCLAVE=$$(kurtosis enclave ls | awk 'NR==2 {print $$1}'); kurtosis enclave rm -f $$ENCLAVE
	kurtosis engine stop

PHONY: sync-lock
sync-lock:
	cargo update -p kona-client
	cd scripts && python sync_lock.py
	# Check build
	# Downgrade the crate that does not exist in op-rs, which was unnecessarily upgraded by cargo update.
	cargo build


