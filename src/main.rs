#![feature(const_trait_impl)]
extern crate core;

use crate::checkpoint::{start_checkpoint_server, LastBlock};
use crate::config::Config;
use crate::derivation::client::l2::L2Client;
use crate::derivation::oracle::lockfree::make_oracle;
use crate::derivation::oracle::new_cache;
use crate::derivation::ChannelInterface;
use crate::polling::start_polling_task;
use crate::webapp::oracle::TracingPreimageIO;
use crate::webapp::start_http_server_task;
use clap::Parser;
use kona_common_proc::client_entry;
use kona_preimage::{CommsClient, PreimageKey};
use log::error;
use lru::LruCache;
use op_alloy_genesis::RollupConfig;
use optimism_derivation::derivation::Derivation;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::metadata::LevelFilter;
use tracing::{info, Level};
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod checkpoint;
mod config;
mod derivation;
mod polling;
mod webapp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    // start tracing
    let filter = filter::EnvFilter::from_default_env()
        .add_directive("optimism_preimage_maker=info".parse()?);
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();
    info!("start optimism preimage-maker");

    let l2_client = L2Client::new(
        config.l2_rollup_node_address.to_string(),
        config.l2_node_address.to_string(),
    );
    let rollup_config = l2_client.rollup_config().await?;
    let chain_id = l2_client.chain_id().await?;

    let (checkpoint_task, saver, last_block) = start_checkpoint_server(config.datadir.as_str())?;

    let (sender, mut receiver) = mpsc::channel::<ChannelInterface>(20);

    // Start HTTP server
    let http_server_task = start_http_server_task(config.http_server_addr.as_str(), sender.clone());

    // Start Polling server
    let polling_task = start_polling_task(last_block, l2_client, sender);

    // Start Derivation host
    let oracle = make_oracle(&config).await;

    while let Some((derivations, reply)) = receiver.recv().await {
        if let Some(reply) = reply {
            let oracle = TracingPreimageIO::new(oracle.clone());
            let mut error = false;
            for derivation in derivations.iter() {
                info!("start derivation {:?}", derivation);
                let result = derivation
                    .verify(chain_id, &rollup_config, oracle.clone())
                    .await;
                match result {
                    Ok(_) => info!(
                        "end derivation claiming number = {}",
                        derivation.l2_block_number
                    ),
                    Err(e) => {
                        tracing::error!(
                            "end derivation claiming number = {} with error = {:?}",
                            derivation.l2_block_number,
                            e
                        );
                        error = true;
                        break;
                    }
                };
            }

            // Return to caller
            let reply_result = if error {
                reply.send(vec![])
            } else {
                reply.send(oracle.into())
            };
            if let Err(err) = reply_result {
                error!("failed to send derivation result: preimage size = {:?}", err.len());
            }
        } else {
            for derivation in derivations.iter() {
                info!("start derivation {:?}", derivation.l2_block_number);
                let result = derivation
                    .verify(chain_id, &rollup_config, oracle.clone())
                    .await;
                match result {
                    Ok(_) => info!(
                        "end derivation claiming number = {}",
                        derivation.l2_block_number
                    ),
                    Err(e) => tracing::error!(
                        "end derivation claiming number = {} with error = {:?}",
                        derivation.l2_block_number,
                        e
                    ),
                };
            }
        };

        // Save last block
        let last_block = LastBlock {
            l2_block_number: derivations.last().unwrap().l2_block_number,
        };
        if let Err(err) = saver.send(last_block).await {
            error!("failed to send to saving last block: {:?}", err);
        }
    }

    http_server_task.abort();
    polling_task.abort();
    checkpoint_task.abort();

    Ok(())
}
