# optimism-preimage-maker
Preimage maker for optimism

## Prerequisites
* [kurtosis-cli v1.6.0](https://docs.kurtosis.com/install/)
* [just](https://github.com/casey/just)

## Quickstart

### Start optimism devnet
```
make chain
make devnet-up
make set-port
make status
```

Wait until the sufficient(several hundred blocks) finalized l2 is found. 

### E2E test

```
make server-up
make test
```
