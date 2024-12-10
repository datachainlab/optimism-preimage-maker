#![feature(const_trait_impl)]
extern crate core;

use std::sync::Arc;
use std::time::Duration;
use optimism_derivation::derivation::Derivation;
use clap::Parser;
use kona_common_proc::client_entry;
use kona_preimage::PreimageKey;
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
use crate::handler::{start_http_server, DerivationState};
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

    let (sender, mut receiver) = mpsc::channel::<(Derivation, Option<oneshot::Sender<Vec<u8>>>)>(100);

    let sender_for_web_server = sender.clone();
    let task = tokio::spawn(async move {
        let derivation_state = DerivationState {
            sender: sender_for_web_server,
        };
        let result = start_http_server("0.0.0.0:10080", derivation_state).await;
        tracing::info!("result {:?}", result);
    });


    let delivery = tokio::spawn(async move {
        loop {
            let sync_status = l2_client.sync_status().await.unwrap();
            //  info!("sync status {:?}", sync_status);
            for n in 0..10 {
                // TODO last saved
                let i = 10 - n;

                let claiming_l2_number = sync_status.finalized_l2.number - i;

                let claiming_l2_hash = l2_client.get_block_by_number(claiming_l2_number).await.unwrap().hash;
                let claiming_output_root = l2_client.output_root_at(claiming_l2_number).await.unwrap();

                let agreed_l2_hash = l2_client.get_block_by_number(sync_status.finalized_l2.number - i - 1).await.unwrap().hash;
                let agreed_output_root = l2_client.output_root_at(sync_status.finalized_l2.number - i - 1).await.unwrap();
                /*
                let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone(), agreed_l2_hash);;
                let oracle = PreimageIO::new(Arc::new(fetcher));
                 */
                let derivation = Derivation::new(
                    sync_status.finalized_l1.hash,
                    agreed_l2_hash,
                    agreed_output_root,
                    claiming_l2_hash,
                    claiming_output_root,
                    claiming_l2_number
                );
                sender.send((derivation, None)).await.unwrap();
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    });

    let cache = new_cache();
    let fetcher = Fetcher::new(global_kv_store.clone(), l1_provider.clone(), blob_provider.clone(), l2_provider.clone());
    let fetcher = Arc::new(RwLock::new(fetcher));
    let (hint_channel, hint_server) = start_hint_server(fetcher.clone());
    let (preimage_channel, preimage_server) = start_preimage_server(fetcher.clone());
    let oracle = PreimageIO::new(cache.clone(), hint_channel, preimage_channel);

    while let Some((derivation, reply)) = receiver.recv().await {
        tracing::info!("start derivation {:?}", derivation);
        match derivation.verify(config.l2_chain_id, &rollup_config, oracle.clone()).await {
            Ok(_) => {
                tracing::info!("end derivation claiming number = {}", derivation.l2_block_number);
                if let Some(reply) = reply{
                    if let Err(err) = reply.send(vec![]) {
                        error!("send reply error = {:?}", err);
                    }
                };
            },
            Err(e) => {
                tracing::error!("end derivation claiming number = {} with error = {:?}", derivation.l2_block_number, e);
                if let Some(reply) = reply{
                    if let Err(err) = reply.send(vec![]) {
                        error!("send reply error = {:?}", err);
                    }
                };
            }
        }

    }

    Ok(())
}
