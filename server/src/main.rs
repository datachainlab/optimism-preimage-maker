#![allow(dead_code)]
use crate::client::l2_client::L2Client;
use crate::collector::PreimageCollector;
use crate::data::file_preimage_repository::FilePreimageRepository;
use crate::derivation::host::single::config::Config;
use crate::derivation::host::single::handler::DerivationConfig;
use crate::web::start_http_server_task;
use crate::web::SharedState;
use anyhow::Context;
use base64::Engine;
use clap::Parser;
use kona_registry::ROLLUP_CONFIGS;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tracing::info;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::client::beacon_client::BeaconClient;
use crate::data::file_finalized_l1_repository::FileFinalizedL1Repository;
use crate::purger::PreimagePurger;

mod client;
mod collector;
mod data;
mod derivation;
mod web;
mod purger;

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
    info!("start optimism-preimage-maker");

    let l2_client = L2Client::new(
        config.l2_rollup_address.to_string(),
        config.l2_node_address.to_string(),
    );
    let beacon_client = BeaconClient::new(
        config.l1_beacon_address.to_string(),
    );
    let chain_id = l2_client.chain_id().await?;
    let (rollup_config, l1_chain_config) = if ROLLUP_CONFIGS.get(&chain_id).is_none() {
        // devnet only
        info!(
            "fetching rollup config and l1 chain config because chain id {} is devnet",
            chain_id
        );
        let l2_config = l2_client.rollup_config().await?;
        let l1_chain_config = config
            .l1_chain_config
            .as_ref()
            .context("l1 chain config is required")?;
        let decoded = base64::engine::general_purpose::STANDARD.decode(l1_chain_config)?;
        let l1_chain_config: kona_genesis::L1ChainConfig = serde_json::from_slice(&decoded)?;
        (Some(l2_config), Some(l1_chain_config))
    } else {
        (None, None)
    };

    let derivation_config = DerivationConfig {
        rollup_config,
        l1_chain_config,
        config: config.clone(),
        l2_chain_id: chain_id,
    };

    // Start preimage collector
    info!("purging ttl = {} sec", config.ttl);
    let ttl = Duration::from_secs(config.ttl);
    let preimage_repository = Arc::new(FilePreimageRepository::new(&config.preimage_dir, ttl).await?);
    let finalized_l1_repository = Arc::new(FileFinalizedL1Repository::new(&config.finalized_l1_dir, ttl)?);
    let collector = PreimageCollector {
        client: Arc::new(l2_client),
        beacon_client: Arc::new(beacon_client),
        config: Arc::new(derivation_config),
        max_distance: config.max_preimage_distance,
        initial_claimed: config.initial_claimed_l2,
        interval_seconds: config.collector_interval_seconds,
        preimage_repository: preimage_repository.clone(),
        finalized_l1_repository: finalized_l1_repository.clone(),
        max_concurrency: config.max_collect_concurrency as usize,
    };
    let collector_task = tokio::spawn(async move {
        collector.start().await;
    });

    let purger = PreimagePurger {
        preimage_repository: preimage_repository.clone(),
        finalized_l1_repository: finalized_l1_repository.clone(),
        interval_seconds:config.purger_interval_seconds,
    };
    let purger_task = tokio::spawn(async move {
        purger.start().await;
    });

    // Start HTTP server
    let http_server_task = start_http_server_task(
        config.http_server_addr.as_str(),
        SharedState {
            preimage_repository,
            finalized_l1_repository,
        },
    );

    // Wait for signal
    select! {
        result = http_server_task => {
            info!("stop http server: {:?}", result);
        }
        result = collector_task => {
            info!("stop collector : {:?}", result);
        }
        result = purger_task => {
            info!("stop purger : {:?}", result);
        }
    }
    info!("shutdown complete");
    Ok(())
}
