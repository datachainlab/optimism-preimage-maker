use std::path::PathBuf;
use alloy_provider::ReqwestProvider;
use alloy_rpc_client::RpcClient;
use alloy_transport_http::Http;
use clap::Parser;
use reqwest::Client;
use serde::Serialize;

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
        default_value = "../optimism/.devnet/rollup.json",
    )]
    pub rollup_config_path: Option<PathBuf>,
}


