SED = $(shell which gsed 2>/dev/null || echo sed)

.PHONY: chain
chain:
	git clone --depth 1 -b v1.13.4 https://github.com/ethereum-optimism/optimism ./chain
	$(SED) -i 's/teku/lodestar\n      cl_image: chainsafe\/lodestar:v1.34.0\n      vc_image: chainsafe\/lodestar:v1.34.0/g' chain/kurtosis-devnet/simple.yaml
	$(SED) -i 's/minimal/minimal\n    electra_fork_epoch: 0/g' chain/kurtosis-devnet/simple.yaml
	# change ethereum-packages
	$(SED) -i 's/83830d44823767af65eda7dfe6b26c87c536c4cf/95b059294110c02613897fd9d3458ce350874bb3/g' chain/kurtosis-devnet/optimism-package-trampoline/kurtosis.yml

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
	@ENCLAVE=$$(kurtosis enclave ls | awk 'NR==2 {print $$1}'); kurtosis enclave rm -f $$ENCLAVE
	kurtosis engine stop

