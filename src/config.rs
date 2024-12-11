use std::path::PathBuf;
use std::sync::{Arc};
use alloy_provider::ReqwestProvider;
use alloy_rpc_client::RpcClient;
use alloy_transport_http::Http;
use anyhow::{anyhow, Result};
use clap::Parser;
use kona_derive_alloy::{OnlineBeaconClient, OnlineBlobProvider};
use kona_host::kv::{DiskKeyValueStore, LocalKeyValueStore, MemoryKeyValueStore, SharedKeyValueStore, SplitKeyValueStore};
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
        visible_alias = "rollup",
        default_value="http://localhost:7545"
    )]
    pub l2_rollup_node_address: String,
    /// Address of L2 JSON-RPC endpoint to use (eth and debug namespace required).
    #[clap(
        long,
        visible_alias = "l2",
        default_value="http://localhost:9545"
    )]
    pub l2_node_address: String,
    /// Address of L1 JSON-RPC endpoint to use (eth and debug namespace required)
    #[clap(
        long,
        visible_alias = "l1",
        default_value="http://localhost:8545"
    )]
    pub l1_node_address: String,
    /// Address of the L1 Beacon API endpoint to use.
    #[clap(
        long,
        visible_alias = "beacon",
        default_value="http://localhost:5052"
    )]
    pub l1_beacon_address: String,
    /// The L2 chain ID of a supported chain. If provided, the host will look for the corresponding
    /// rollup config in the superchain registry.
    #[clap(
        long,
        default_value="901"
    )]
    pub l2_chain_id: u64,
    /// Path to rollup config. If provided, the host will use this config instead of attempting to
    /// look up the config in the superchain registry.
    #[clap(
        long,
        default_value="../optimism/.devnet/rollup.json"
    )]
    pub rollup_config_path: Option<PathBuf>,

    /// Path to rollup config. If provided, the host will use this config instead of attempting to
    /// look up the config in the superchain registry.
    #[clap(
        long,
        default_value="0.0.0.0:10080"
    )]
    pub http_server_addr: String,
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
            self.l1_beacon_address.clone()
        );
        let mut blob_provider = OnlineBlobProvider::new(beacon_client, None, None);
        blob_provider
            .load_configs()
            .await
            .map_err(|e| anyhow!("Failed to load blob provider configuration: {e}"))?;
        let l1_provider = http_provider(
            &self.l1_node_address
        );
        let l2_provider = http_provider(
            &self.l2_node_address
        );

        Ok((l1_provider, blob_provider, l2_provider))
    }

    pub fn construct_kv_store(&self) -> SharedKeyValueStore {
        let kv_store= DiskKeyValueStore::new(".data/remote".into());
        //let kv_store = MemoryKeyValueStore::new();
        let kv_store: SharedKeyValueStore = Arc::new(RwLock::new(kv_store));
        kv_store
    }
}



fn http_provider(url: &str) -> ReqwestProvider {
    let url = url.parse().unwrap();
    let http = Http::<Client>::new(url);
    ReqwestProvider::new(RpcClient::new(http, true))
}