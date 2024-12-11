#![feature(const_trait_impl)]
extern crate core;

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use optimism_derivation::derivation::Derivation;
use clap::Parser;
use kona_common_proc::client_entry;
use kona_preimage::{CommsClient, PreimageKey};
use log::error;
use lru::LruCache;
use op_alloy_genesis::RollupConfig;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, Level};
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::config::Config;
use crate::derivation::ChannelInterface;
use crate::derivation::oracle::lockfree::client::PreimageIO;
use crate::derivation::oracle::lockfree::fetcher::Fetcher;
use crate::derivation::oracle::lockfree::server::{start_hint_server, start_preimage_server};
use crate::derivation::oracle::new_cache;
use crate::polling::start_polling_task;
use crate::webapp::start_http_server_task;
use crate::webapp::oracle::{PreimageTraceable, TracingPreimageIO};

mod config;
mod polling;
mod webapp;
mod derivation;

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


    let (sender, mut receiver) = mpsc::channel::<ChannelInterface>(100);

    // Create tasks
    let http_server_task = start_http_server_task(config.http_server_addr.as_str(), sender.clone());
    let polling_task = start_polling_task(config.l2_rollup_node_address.as_str(), config.l2_node_address.as_str(), sender);

    // Start Derivation host
    let cache = new_cache();
    let global_kv_store = config.construct_kv_store();
    let (l1_provider, blob_provider, l2_provider) = config.create_providers().await?;
    let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone());
    let fetcher = Arc::new(RwLock::new(fetcher));
    let (hint_channel, hint_server) = start_hint_server(fetcher.clone());
    let (preimage_channel, preimage_server) = start_preimage_server(fetcher.clone());
    let oracle = PreimageIO::new(cache.clone(), hint_channel, preimage_channel);

    while let Some((derivation, reply)) = receiver.recv().await {
        info!("start derivation {:?}", derivation);
        let result = if let Some(reply) = reply {
            let oracle = TracingPreimageIO::new(cache.clone());
            let result = derivation.verify(config.l2_chain_id, &rollup_config, oracle.clone()).await;
            match result {
                Ok(v) => {
                    if let Err(err) = reply.send(oracle.preimages()) {
                        error!("send reply error = {:?}", err);
                    }
                    Ok(v)
                },
                Err(e) => {
                    if let Err(err) = reply.send(vec![]) {
                        error!("send reply error = {:?}", err);
                    }
                    Err(e)
                }
            }
        }else {
            derivation.verify(config.l2_chain_id, &rollup_config, oracle.clone()).await
        };
        match result {
            Ok(_) => info!("end derivation claiming number = {}", derivation.l2_block_number),
            Err(e) => tracing::error!("end derivation claiming number = {} with error = {:?}", derivation.l2_block_number, e)
        };
    }

    http_server_task.abort();
    polling_task.abort();

    Ok(())
}
