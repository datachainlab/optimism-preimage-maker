//! This module contains all CLI-specific code for the single chain entrypoint.

use clap::Parser;
use serde::Serialize;

/// The host binary CLI application arguments.
#[derive(Default, Parser, Serialize, Clone, Debug)]
pub struct Config {
    /// Address of L2 JSON-RPC endpoint to use (eth and debug namespace required).
    #[clap(long, visible_alias = "l2", default_value = "http://localhost:61916")]
    pub l2_node_address: String,
    /// Address of L1 JSON-RPC endpoint to use (eth and debug namespace required)
    #[clap(long, visible_alias = "l1", default_value = "http://localhost:61048")]
    pub l1_node_address: String,
    /// Address of the L1 Beacon API endpoint to use.
    #[clap(
        long,
        visible_alias = "beacon",
        default_value = "http://localhost:61058"
    )]
    pub l1_beacon_address: String,
    /// Address of L2 JSON-RPC endpoint to use (eth and debug namespace required).
    #[clap(
        long,
        visible_alias = "rollup",
        default_value = "http://localhost:61923"
    )]
    pub l2_rollup_address: String,
    /// Path to rollup config. If provided, the host will use this config instead of attempting to
    /// look up the config in the superchain registry.
    #[clap(long, default_value = "0.0.0.0:10080")]
    pub http_server_addr: String,

    /// Optional L1 chain config base64 json string. (this is only required for devnet)
    #[clap(long)]
    pub l1_chain_config: Option<String>,

    /// preimage directory if specified. (ex. .preimage)
    #[clap(long, default_value = ".preimage")]
    pub preimage_dir: String,

    /// Max preimage distance ( from agreed to claimed) per one call
    #[clap(long, default_value = "100")]
    pub max_preimage_distance: u64,

    /// Initial claimed l2 block number that is used when no preimage is created.
    #[clap(long)]
    pub initial_claimed_l2: u64,
   
    /// Max concurrency of preimage collector.
    #[clap(long, default_value = "10")]
    pub max_collect_concurrency: u64
}
