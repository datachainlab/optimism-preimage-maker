//! [SingleChainHostCli]'s [HostOrchestrator] + [DetachedHostOrchestrator] implementations.

use crate::host::single::config::Config;
use crate::host::single::local_kv::LocalKeyValueStore;
use crate::host::single::trace::{encode_to_bytes, TracingKeyValueStore};
use crate::transport::Http;
use alloy_primitives::B256;
use alloy_provider::RootProvider;
use alloy_rpc_client::RpcClient;
use anyhow::Result;
use kona_genesis::RollupConfig;
use kona_host::single::{
    SingleChainHintHandler, SingleChainHost, SingleChainHostError, SingleChainProviders,
};
use kona_host::{MemoryKeyValueStore, OnlineHostBackend, PreimageServer, SplitKeyValueStore};
use kona_preimage::{
    BidirectionalChannel, HintReader, HintWriter, NativeChannel, OracleReader, OracleServer,
    PreimageKey,
};
use kona_proof::boot::L2_ROLLUP_CONFIG_KEY;
use kona_proof::HintType;
use kona_providers_alloy::{OnlineBeaconClient, OnlineBlobProvider};
use op_alloy_network::{Network, Optimism};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::RwLock;

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

    /// Creates the providers required for the host backend.
    async fn create_providers(&self) -> Result<SingleChainProviders, SingleChainHostError> {
        let l1_provider = Self::http_provider(&self.config.l1_node_address);
        let blob_provider = OnlineBlobProvider::init(OnlineBeaconClient::new_http(
            self.config.l1_beacon_address.clone(),
        ))
        .await;
        let l2_provider = Self::http_provider::<Optimism>(&self.config.l2_node_address);
        Ok(SingleChainProviders {
            l1: l1_provider,
            blobs: blob_provider,
            l2: l2_provider,
        })
    }

    fn http_provider<N: Network>(url: &str) -> RootProvider<N> {
        let url = url.parse().unwrap();
        let http = Http::<Client>::new(url);
        RootProvider::new(RpcClient::new(http, true))
    }

    async fn run_server(
        &self,
        preimage_host: NativeChannel,
        hint_host: NativeChannel,
        backend: OnlineHostBackend<SingleChainHost, SingleChainHintHandler>,
    ) -> Result<()> {
        PreimageServer::new(
            OracleServer::new(preimage_host),
            HintReader::new(hint_host),
            Arc::new(backend),
        )
        .start()
        .await
        .map_err(|e| e.into())
    }

    async fn run_client(
        &self,
        preimage_client: NativeChannel,
        hint_client: NativeChannel,
    ) -> Result<()> {
        kona_client::single::run(
            OracleReader::new(preimage_client),
            HintWriter::new(hint_client),
        )
        .await
        .map_err(|e| e.into())
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
        let providers = self.create_providers().await?;
        let backend =
            OnlineHostBackend::new(cfg, kv_store.clone(), providers, SingleChainHintHandler)
                .with_proactive_hint(HintType::L2PayloadWitness);

        let server_task = self.run_server(preimage.host, hint.host, backend);
        let client_task = self.run_client(preimage.client, hint.client);
        tokio::try_join!(server_task, client_task)?;

        // Collect preimages from the kv store
        let mut used = {
            let mut lock = kv_store.write().await;
            std::mem::take(&mut lock.used)
        };
        let local_key = PreimageKey::new_local(L2_ROLLUP_CONFIG_KEY.to());
        let roll_up_config_json = serde_json::to_vec(&self.rollup_config)?;
        used.insert(local_key, roll_up_config_json);

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
}
