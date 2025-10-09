# optimism-preimage-maker
Preimage maker for optimism

## Prerequisites
* [kurtosis-cli v1.11.1](https://docs.kurtosis.com/install-historical/)
* [just](https://github.com/casey/just)

## Quickstart

### Start optimism devnet
```
make chain
make devnet-up
make set-port
make wait
```

Wait until the finalized l2 is found. 

### E2E test

```
make server-up
make test
```
