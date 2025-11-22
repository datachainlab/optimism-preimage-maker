use std::sync::Arc;
use crate::derivation::host::single::config::Config;
use crate::web::{start_http_server_task, };
use anyhow::Context;
use base64::Engine;
use clap::Parser;
use kona_registry::ROLLUP_CONFIGS;
use tokio::{select, try_join};
use crate::client::l2_client::L2Client;
use tracing::info;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::web::SharedState;
use crate::collector::PreimageCollector;
use crate::data::file_preimage_repository::FilePreimageRepository;
use crate::derivation::host::single::handler::DerivationConfig;

mod client;
mod web;
mod collector;
mod derivation;
mod data;

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
        config.l2_rollup_address.to_string(),
        config.l2_node_address.to_string(),
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
    let preimage_repository = Arc::new(FilePreimageRepository::new(&config.preimage_dir).await?);
    let collector = PreimageCollector {
        client: l2_client,
        config: derivation_config.clone(),
        chunk: config.max_preimage_distance,
        initial_claimed: config.initial_claimed_l2,
        preimage_repository: preimage_repository.clone(),
    };
    let collector_task = tokio::spawn(async move {
        collector.start().await;
    });

    // Start HTTP server
    let http_server_task = start_http_server_task(
        config.http_server_addr.as_str(),
        SharedState {
           preimage_repository
        }
    );

    // Wait for signal
    select! {
        result = http_server_task => {
            info!("stop http server: {:?}", result);
        }
        result = collector_task => {
            info!("stop collector : {:?}", result);
        }
    };
    info!("shutdown complete");
    Ok(())
}
