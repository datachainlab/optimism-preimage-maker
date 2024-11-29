use std::path::PathBuf;
use std::sync::{Arc};
use alloy_provider::ReqwestProvider;
use alloy_rpc_client::RpcClient;
use alloy_transport_http::Http;
use anyhow::{anyhow, Result};
use clap::Parser;
use kona_derive_alloy::{OnlineBeaconClient, OnlineBlobProvider};
use kona_host::kv::{LocalKeyValueStore, MemoryKeyValueStore, SharedKeyValueStore, SplitKeyValueStore};
use reqwest::Client;
use serde::Serialize;
use tokio::sync::RwLock;

const ABOUT: &str = "
optimism-preimage-maker is a CLI application that runs the Kona derivation.
";
#[derive(Default, Parser, Serialize, Clone, Debug)]
#[command(about = ABOUT, version)]
pub struct Config {
    /// Address of L2 JSON-RPC endpoint to use (eth and debug namespace required).
    #[clap(
        long,
        visible_alias = "l2",
        requires = "l1_node_address",
        requires = "l1_beacon_address"
    )]
    pub l2_node_address: Option<String>,
    /// Address of L1 JSON-RPC endpoint to use (eth and debug namespace required)
    #[clap(
        long,
        visible_alias = "l1",
        requires = "l2_node_address",
        requires = "l1_beacon_address"
    )]
    pub l1_node_address: Option<String>,
    /// Address of the L1 Beacon API endpoint to use.
    #[clap(
        long,
        visible_alias = "beacon",
        requires = "l1_node_address",
        requires = "l2_node_address"
    )]
    pub l1_beacon_address: Option<String>,
    /// The L2 chain ID of a supported chain. If provided, the host will look for the corresponding
    /// rollup config in the superchain registry.
    #[clap(
        long,
    )]
    pub l2_chain_id: u64,
    /// Path to rollup config. If provided, the host will use this config instead of attempting to
    /// look up the config in the superchain registry.
    #[clap(
        long,
    )]
    pub rollup_config_path: Option<PathBuf>,
}

impl Config {
    pub async fn create_providers(
        &self,
    ) -> Result<(
        ReqwestProvider,
        OnlineBlobProvider<OnlineBeaconClient>,
        ReqwestProvider,
    )> {
        let beacon_client = OnlineBeaconClient::new_http(
            self.l1_beacon_address.clone().ok_or(anyhow!("Beacon API URL must be set"))?,
        );
        let mut blob_provider = OnlineBlobProvider::new(beacon_client, None, None);
        blob_provider
            .load_configs()
            .await
            .map_err(|e| anyhow!("Failed to load blob provider configuration: {e}"))?;
        let l1_provider = http_provider(
            self.l1_node_address.as_ref().ok_or(anyhow!("Provider must be set"))?,
        );
        let l2_provider = http_provider(
            self.l2_node_address.as_ref().ok_or(anyhow!("L2 node address must be set"))?,
        );

        Ok((l1_provider, blob_provider, l2_provider))
    }

    pub fn construct_kv_store(&self) -> SharedKeyValueStore {
        let mem_kv_store = MemoryKeyValueStore::new();
        let split_kv_store = SplitKeyValueStore::new(mem_kv_store.clone(), mem_kv_store.clone());
        let kv_store: SharedKeyValueStore = Arc::new(RwLock::new(split_kv_store));
        kv_store
    }
}



fn http_provider(url: &str) -> ReqwestProvider {
    let url = url.parse().unwrap();
    let http = Http::<Client>::new(url);
    ReqwestProvider::new(RpcClient::new(http, true))
}