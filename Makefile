.PHONY: chain
chain:
	git clone --depth 1 -b v1.13.1 https://github.com/ethereum-optimism/optimism ./chain
	sed 's/teku/lodestar/g' chain/kurtosis-devnet/simple.yaml > chain/kurtosis-devnet/simple.yaml.tmp
	sed 's/minimal/minimal\n    electra_fork_epoch: 0/g' chain/kurtosis-devnet/simple.yaml > chain/kurtosis-devnet/simple.yaml.tmp2
	mv chain/kurtosis-devnet/simple.yaml.tmp2 chain/kurtosis-devnet/simple.yaml

.PHONY: devnet-up
devnet-up:
	cd chain/kurtosis-devnet && just simple-devnet

.PHONY: set-port
set-port:
	scripts/port.sh

.PHONY: status
status:
	@PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	curl -X POST localhost:$$PORT -d '{"method":"optimism_syncStatus", "jsonrpc": "2.0", "id":1, "params":[]}' -H "Content-Type: application/json" | jq .result

.PHONY: wait
wait:
	./scripts/wait.sh

.PHONY: server-up
server-up:
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L1_GETH_PORT=$$(jq -r '.l1GethPort' hostPort.json);\
	L1_BEACON_PORT=$$(jq -r '.l1BeaconPort' hostPort.json);\
	cargo run --release --bin=optimism-preimage-maker -- \
		--rollup=http://localhost:$$L2_ROLLUP_PORT \
		--l2=http://localhost:$$L2_GETH_PORT \
		--l1=http://localhost:$$L1_GETH_PORT \
		--beacon=http://localhost:$$L1_BEACON_PORT

.PHONY: test
test:
	@L2_ROLLUP_PORT=$$(jq -r '.l2RollupPort' hostPort.json);\
	L2_GETH_PORT=$$(jq -r '.l2GethPort' hostPort.json);\
	L2_ROLLUP_PORT=$$L2_ROLLUP_PORT L2_GETH_PORT=$$L2_GETH_PORT cargo test --manifest-path=./server/Cargo.toml

.PHONY: devnet-down
devnet-down:
	kurtosis engine restart
	@ENCLAVE=$$(kurtosis enclave ls | awk 'NR==2 {print $$1}'); kurtosis enclave stop $$ENCLAVE
	kurtosis engine stop
	docker network rm kt-simple-devnet

