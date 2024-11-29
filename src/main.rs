extern crate core;

use std::sync::{Arc, RwLock};
use optimism_derivation::derivation::Derivation;
use clap::Parser;
use kona_host::fetcher::Fetcher;
use op_alloy_genesis::RollupConfig;
use serde::Serialize;
use crate::config::Config;
use crate::l2::client::L2Client;
use crate::oracle::PreimageIO;

mod oracle;
mod l2;
mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    let config = Config::parse();

    let rollup_config = match &config.rollup_config_path {
        None => RollupConfig::from_l2_chain_id(config.l2_chain_id).unwrap(),
        Some(path) => {
            let json = std::fs::read(path.clone())?;
            serde_json::from_slice(json.as_slice())?
        }
    };

    let l2_client = L2Client::default();
    let sync_status = l2_client.sync_status().await?;

    // TODO last saved
    let agreed_l2_hash= l2_client.get_block_by_number(sync_status.finalized_l2.number - 1).await?.hash;
    let agreed_output_root = l2_client.output_root_at(sync_status.finalized_l2.number - 1).await?;

    let claiming_l2_hash= l2_client.get_block_by_number(sync_status.finalized_l2.number).await?.hash;
    let claiming_output_root = l2_client.output_root_at(sync_status.finalized_l2.number).await?;

    let kv_store = config.construct_kv_store();
    let (l1_provider, blob_provider, l2_provider) = config.create_providers().await?;
    let fetcher = Fetcher::new(kv_store.clone(), l1_provider, blob_provider, l2_provider, claiming_l2_hash);
    let oracle = PreimageIO {
        fetcher: Arc::new(RwLock::new(fetcher))
    };

    Derivation::new(
        sync_status.finalized_l1.hash,
        agreed_l2_hash,
        agreed_output_root,
        claiming_l2_hash,
        claiming_output_root,
        sync_status.finalized_l2.number
    ).verify(config.l2_chain_id, &rollup_config, Arc::new(oracle)).await?;

    Ok(())
}
