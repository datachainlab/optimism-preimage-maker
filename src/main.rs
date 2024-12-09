#![feature(const_trait_impl)]
extern crate core;

use std::sync::Arc;
use std::time::Duration;
use optimism_derivation::derivation::Derivation;
use clap::Parser;
use kona_common_proc::client_entry;
use kona_preimage::PreimageKey;
use lru::LruCache;
use op_alloy_genesis::RollupConfig;
use serde::Serialize;
use tokio::sync::{oneshot, RwLock, };
use tracing::Level;
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::config::Config;
use crate::l2::client::L2Client;
use crate::oracle::lockfree::client::PreimageIO;
use crate::oracle::lockfree::fetcher::Fetcher;
use crate::oracle::lockfree::server::{start_hint_server, start_preimage_server};
use crate::oracle::new_cache;

mod l2;
mod config;
mod oracle;
mod handler;

#[tokio::main]
async fn main() -> anyhow::Result<()>{
    let config = Config::parse();

    // start tracing
    let filter = filter::EnvFilter::from_default_env().add_directive("optimism_preimage_maker=info".parse()?);
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();
    tracing::info!("start optimism preimage-maker");

    let rollup_config = match &config.rollup_config_path {
        None => RollupConfig::from_l2_chain_id(config.l2_chain_id).unwrap(),
        Some(path) => {
            let json = std::fs::read(path.clone())?;
            serde_json::from_slice(json.as_slice())?
        }
    };


    let global_kv_store = config.construct_kv_store();
    let (l1_provider, blob_provider, l2_provider) = config.create_providers().await?;

    let l2_client = L2Client::new(config.l2_rollup_node_address.clone(), config.l2_node_address.clone());

    let cache = new_cache();
    let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone());
    let fetcher = Arc::new(RwLock::new(fetcher));

    let (hint_channel, hint_server) = start_hint_server(fetcher.clone());
    let (preimage_channel, preimage_server) = start_preimage_server(fetcher.clone());
    let oracle = Arc::new(PreimageIO::new(cache.clone(), hint_channel, preimage_channel));

    loop {
        let sync_status = l2_client.sync_status().await?;
        for n in 0..10 {
            // TODO last saved
            let i = 10 - n;

            let claiming_l2_number = sync_status.finalized_l2.number - i;

            let claiming_l2_hash = l2_client.get_block_by_number(claiming_l2_number).await?.hash;
            let claiming_output_root = l2_client.output_root_at(claiming_l2_number).await?;

            let agreed_l2_hash = l2_client.get_block_by_number(sync_status.finalized_l2.number - i - 1).await?.hash;
            let agreed_output_root = l2_client.output_root_at(sync_status.finalized_l2.number - i - 1).await?;

            /*
            let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone(), agreed_l2_hash);;
            let oracle = PreimageIO::new(Arc::new(fetcher));
             */
            tracing::info!("start derivation claiming number = {}", claiming_l2_number);
            Derivation::new(
                sync_status.finalized_l1.hash,
                agreed_l2_hash,
                agreed_output_root,
                claiming_l2_hash,
                claiming_output_root,
                claiming_l2_number
            ).verify(config.l2_chain_id, &rollup_config, oracle.clone()).await?;
            tracing::info!("end derivation claiming number = {}", claiming_l2_number);

        }

        tokio::time::sleep(Duration::from_secs(20)).await;
    }

    Ok(())
}
