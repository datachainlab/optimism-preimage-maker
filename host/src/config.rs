use std::path::PathBuf;
use std::sync::Arc;
use alloy_provider::ReqwestProvider;
use anyhow::anyhow;
use clap::Parser;
use kona_derive_alloy::{OnlineBeaconClient, OnlineBlobProvider};
use kona_host::kv::{DiskKeyValueStore, LocalKeyValueStore, MemoryKeyValueStore, SharedKeyValueStore, SplitKeyValueStore};
use serde::Serialize;
use tokio::sync::RwLock;
use crate::util;

const ABOUT: &str = "
kona-host is a CLI application that runs the Kona pre-image server and client program. The host
can run in two modes: server mode and native mode. In server mode, the host runs the pre-image
server and waits for the client program in the parent process to request pre-images. In native
mode, the host runs the client program in a child process with the pre-image server in the primary
thread.
";

/// The host binary CLI application arguments.
#[derive(Default, Parser, Serialize, Clone, Debug)]
#[command(about = ABOUT, version)]
pub struct Config {
    #[clap(
        long,
        visible_alias = "l2",
        default_value="http://localhost:9545"
    )]
    pub l2_node_address: Option<String>,
    /// Address of L1 JSON-RPC endpoint to use (eth and debug namespace required)
    #[clap(
        long,
        visible_alias = "l1",
        default_value = "http://localhost:8545",
    )]
    pub l1_node_address: Option<String>,
    /// Address of the L1 Beacon API endpoint to use.
    #[clap(
        long,
        visible_alias = "beacon",
        default_value = "http://localhost:5052",
    )]
    pub l1_beacon_address: Option<String>,
    /// The Data Directory for preimage data storage. Optional if running in online mode,
    /// required if running in offline mode.
    #[clap(
        long,
        visible_alias = "db",
        default_value = ".data",
    )]
    pub data_dir: Option<PathBuf>,
    /// Run the specified client program natively as a separate process detached from the host.
    #[clap(long,
        default_value = "target/debug/optimism-preimage-maker",
    )]
    pub exec: String,
}

impl Config {

    /// Creates the providers associated with the [kona_host::HostCli] configuration.
    ///
    /// ## Returns
    /// - A [ReqwestProvider] for the L1 node.
    /// - An [OnlineBlobProvider] for the L1 beacon node.
    /// - A [ReqwestProvider] for the L2 node.
    pub async fn create_providers(
        &self,
    ) -> anyhow::Result<(ReqwestProvider, OnlineBlobProvider<OnlineBeaconClient>, ReqwestProvider)> {
        let beacon_client = OnlineBeaconClient::new_http(
            self.l1_beacon_address.clone().ok_or(anyhow!("Beacon API URL must be set"))?,
        );
        let mut blob_provider = OnlineBlobProvider::new(beacon_client, None, None);
        blob_provider
            .load_configs()
            .await
            .map_err(|e| anyhow!("Failed to load blob provider configuration: {e}"))?;
        let l1_provider = util::http_provider(
            self.l1_node_address.as_ref().ok_or(anyhow!("Provider must be set"))?,
        );
        let l2_provider = util::http_provider(
            self.l2_node_address.as_ref().ok_or(anyhow!("L2 node address must be set"))?,
        );

        Ok((l1_provider, blob_provider, l2_provider))
    }

    pub fn construct_kv_store(&self) -> SharedKeyValueStore {
        let kv_store: SharedKeyValueStore = if let Some(ref data_dir) = self.data_dir {
            let disk_kv_store = DiskKeyValueStore::new(data_dir.clone());
            Arc::new(RwLock::new(disk_kv_store))
        } else {
            let mem_kv_store = MemoryKeyValueStore::new();
            Arc::new(RwLock::new(mem_kv_store))
        };
        kv_store
    }
}