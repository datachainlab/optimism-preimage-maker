use crate::host::single::config::Config;
use crate::server::{start_http_server_task, DerivationState};
use base64::Engine;
use clap::Parser;
use kona_registry::ROLLUP_CONFIGS;
use l2_client::L2Client;
use tracing::info;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod host;
pub mod l2_client;
mod server;

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
        let l1_chain_config = config.l1_chain_config.as_ref().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(l1_chain_config)?;
        let l1_chain_config: kona_genesis::L1ChainConfig = serde_json::from_slice(&decoded)?;
        (Some(l2_config), Some(l1_chain_config))
    } else {
        (None, None)
    };

    // Start HTTP server
    let http_server_task = start_http_server_task(
        config.http_server_addr.as_str(),
        DerivationState {
            rollup_config,
            l1_chain_config,
            config: config.clone(),
            l2_chain_id: chain_id,
        },
    );

    let result = http_server_task.await;
    info!("server result : {:?}", result);

    Ok(())
}
