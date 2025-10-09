//! Contains a concrete implementation of the [KeyValueStore] trait that stores data on disk,
//! using the [SingleChainHostCli] config.

use alloy_eips::eip7840::BlobParams;
use std::collections::BTreeMap;
use crate::host::single::handler::DerivationRequest;
use alloy_primitives::{B256, U256};
use anyhow::Result;
use kona_genesis::L1ChainConfig;
use kona_host::KeyValueStore;
use kona_preimage::PreimageKey;
use kona_proof::boot::{
    L1_HEAD_KEY, L2_CHAIN_ID_KEY, L2_CLAIM_BLOCK_NUMBER_KEY, L2_CLAIM_KEY, L2_OUTPUT_ROOT_KEY,
    L2_ROLLUP_CONFIG_KEY, L1_CONFIG_KEY,
};

/// A simple, synchronous key-value store that returns data from a [SingleChainHostCli] config.
#[derive(Debug)]
pub struct LocalKeyValueStore {
    cfg: DerivationRequest,
}

impl LocalKeyValueStore {
    /// Create a new [LocalKeyValueStore] with the given [SingleChainHostCli] config.
    pub const fn new(cfg: DerivationRequest) -> Self {
        Self { cfg }
    }
}

impl KeyValueStore for LocalKeyValueStore {
    fn get(&self, key: B256) -> Option<Vec<u8>> {
        let preimage_key = PreimageKey::try_from(*key).ok()?;
        match preimage_key.key_value() {
            L1_HEAD_KEY => Some(self.cfg.l1_head_hash.to_vec()),
            L2_OUTPUT_ROOT_KEY => Some(self.cfg.agreed_l2_output_root.to_vec()),
            L2_CLAIM_KEY => Some(self.cfg.l2_output_root.to_vec()),
            L2_CLAIM_BLOCK_NUMBER_KEY => Some(self.cfg.l2_block_number.to_be_bytes().to_vec()),
            L2_CHAIN_ID_KEY => Some(self.cfg.l2_chain_id.to_be_bytes().to_vec()),
            L2_ROLLUP_CONFIG_KEY => {
                let rollup_config = self.cfg.rollup_config.clone();
                let serialized = serde_json::to_vec(&rollup_config).ok()?;
                Some(serialized)
            }
            L1_CONFIG_KEY => {
                let l1_config = L1ChainConfig {
                    chain_id: 3151908,
                    homestead_block: Some(0),
                    dao_fork_block: Some(0),
                    dao_fork_support: true,
                    eip150_block: Some(0),
                    eip155_block: Some(0),
                    eip158_block: Some(0),
                    byzantium_block: Some(0),
                    constantinople_block: Some(0),
                    petersburg_block: Some(0),
                    istanbul_block: Some(0),
                    muir_glacier_block: Some(0),
                    berlin_block: Some(0),
                    london_block: Some(0),
                    arrow_glacier_block: Some(0),
                    gray_glacier_block: Some(0),
                    shanghai_time: Some(0),
                    cancun_time: Some(0),
                    prague_time: Some(0),
                    osaka_time: Some(0),
                    terminal_total_difficulty: Some(U256::from(0)),
                    terminal_total_difficulty_passed: true,
                    deposit_contract_address: Some(
                        "0x00000000219ab540356cbb839cbe05303d7705fa"
                            .parse()
                            .unwrap(),
                    ),
                    blob_schedule:   BTreeMap::from([
                        (alloy_hardforks::EthereumHardfork::Cancun.name().to_string(), BlobParams::cancun()),
                        (alloy_hardforks::EthereumHardfork::Prague.name().to_string(), BlobParams::prague()),
                        (alloy_hardforks::EthereumHardfork::Osaka.name().to_string(), BlobParams::osaka()),
                    ]),
                    ..Default::default()
                };
                let serialized = serde_json::to_vec(&l1_config).ok()?;
                Some(serialized)
            }
            _ => None,
        }
    }

    fn set(&mut self, _: B256, _: Vec<u8>) -> Result<()> {
        unreachable!("LocalKeyValueStore is read-only")
    }
}
