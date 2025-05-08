//! [SingleChainHostCli]'s [HostOrchestrator] + [DetachedHostOrchestrator] implementations.

use crate::host::single::config::Config;
use crate::host::single::local_kv::LocalKeyValueStore;
use crate::host::single::trace::{encode_to_bytes, TracingKeyValueStore};
use alloy_primitives::B256;
use anyhow::Result;
use kona_genesis::RollupConfig;
use kona_host::single::{SingleChainHintHandler, SingleChainHost};
use kona_host::{MemoryKeyValueStore, OnlineHostBackend, PreimageServer, SplitKeyValueStore};
use kona_preimage::{
    BidirectionalChannel, HintReader, HintWriter, NativeChannel, OracleReader, OracleServer,
};
use kona_proof::HintType;
use std::sync::Arc;
use kona_client::fpvm_evm::FpvmOpEvmFactory;
use tokio::sync::RwLock;
use tokio::task;

#[derive(Debug, Clone)]
pub struct DerivationRequest {
    pub config: Config,
    pub rollup_config: RollupConfig,
    pub l2_chain_id: u64,
    /// for L2 derivation
    pub agreed_l2_head_hash: B256,
    pub agreed_l2_output_root: B256,
    pub l1_head_hash: B256,
    pub l2_output_root: B256,
    pub l2_block_number: u64,
}

impl DerivationRequest {
    fn create_key_value_store(&self) -> Result<Arc<RwLock<TracingKeyValueStore>>> {
        // Only memory store is traceable
        // Using disk causes insufficient blob preimages in ELC because the already stored data is not traceable
        let local_kv_store = LocalKeyValueStore::new(self.clone());
        let mem_kv_store = MemoryKeyValueStore::new();
        let split_kv_store = SplitKeyValueStore::new(local_kv_store, mem_kv_store);
        Ok(Arc::new(RwLock::new(TracingKeyValueStore::new(Box::new(
            split_kv_store,
        )))))
    }

    async fn run_client_native(
        hint_reader: HintWriter<NativeChannel>,
        oracle_reader: OracleReader<NativeChannel>,
        evm_factory: FpvmOpEvmFactory<NativeChannel>,
    ) -> Result<()> {
        kona_client::single::run(oracle_reader, hint_reader, evm_factory)
            .await
            .map_err(Into::into)
    }

    pub async fn start(&self) -> Result<Vec<u8>> {
        let hint = BidirectionalChannel::new()?;
        let preimage = BidirectionalChannel::new()?;
        let kv_store = self.create_key_value_store()?;
        let cfg = SingleChainHost {
            l1_node_address: Some(self.config.l1_node_address.clone()),
            l2_node_address: Some(self.config.l2_node_address.clone()),
            l1_beacon_address: Some(self.config.l1_beacon_address.clone()),
            l1_head: self.l1_head_hash,
            agreed_l2_output_root: self.agreed_l2_output_root,
            agreed_l2_head_hash: self.agreed_l2_head_hash,
            claimed_l2_output_root: self.l2_output_root,
            claimed_l2_block_number: self.l2_block_number,
            ..Default::default()
        };
        let providers = cfg.create_providers().await?;
        let backend =
            OnlineHostBackend::new(cfg, kv_store.clone(), providers, SingleChainHintHandler)
                .with_proactive_hint(HintType::L2PayloadWitness);

        let server_task = task::spawn(
            PreimageServer::new(
                OracleServer::new(preimage.host),
                HintReader::new(hint.host),
                Arc::new(backend),
            )
            .start(),
        );
        let client_task = task::spawn(Self::run_client_native(
            HintWriter::new(hint.client.clone()),
            OracleReader::new(preimage.client.clone()),
            FpvmOpEvmFactory::new(HintWriter::new(hint.client), OracleReader::new(preimage.client)),
        ));

        let (_, client_result) = tokio::try_join!(server_task, client_task)?;
        match client_result {
            Ok(_) => {
                let used = {
                    let mut lock = kv_store.write().await;
                    std::mem::take(&mut lock.used)
                };
                let entry_size = used.len();
                let preimage = encode_to_bytes(used);
                let preimage_bytes: Vec<u8> = preimage.into_vec().unwrap();
                tracing::info!(
                    "Preimage entry: {}, size: {}",
                    entry_size,
                    preimage_bytes.len()
                );
                Ok(preimage_bytes)
            }
            Err(e) => Err(e),
        }
    }
}
