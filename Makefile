.PHONY: chain
chain:
	git clone --depth 1 -b v1.11.2 https://github.com/ethereum-optimism/optimism ./chain
	sed 's/teku/lodestar/g' chain/kurtosis-devnet/simple.yaml > chain/kurtosis-devnet/simple.yaml.tmp
	mv chain/kurtosis-devnet/simple.yaml.tmp chain/kurtosis-devnet/simple.yaml

.PHONY: devnet-up
devnet-up:
	cd kurtosis-devnet
	just simple-devnet

.PHONY: devnet-down
devnet-down:
	kurtosis engine restart
	kurtosis enclave ls | awk 'NR==2 {print $1}' | kurtosis enclave stop
	kurtosis engine stop
	docker network rm kt-simple-devnet

.PHONY: status
status:
	curl -X POST localhost:7545 -d '{"method":"optimism_syncStatus", "jsonrpc": "2.0", "id":1, "params":[]}' -H "Content-Type: application/json" | jq .result.finalized_l2

