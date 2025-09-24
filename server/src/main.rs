use crate::host::single::config::Config;
use crate::server::{start_http_server_task, DerivationState};
use crate::transport::http_proxy::new_cache;
use crate::transport::metrics::Metrics;
use clap::Parser;
use l2_client::L2Client;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod host;
pub mod l2_client;
mod server;
mod transport;

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
    let cache = if config.cache_size > 0 {
        info!("enable rpc cache with size {}", config.cache_size);
        Some(new_cache(config.cache_size))
    } else {
        info!("disable rpc cache with size");
        None
    };

    let l2_client = L2Client::new(
        config.l2_rollup_address.to_string(),
        config.l2_node_address.to_string(),
    );
    let rollup_config = l2_client.rollup_config().await?;
    let chain_id = l2_client.chain_id().await?;

    // Start HTTP server
    let http_server_task = start_http_server_task(
        config.http_server_addr.as_str(),
        DerivationState {
            rollup_config: rollup_config.clone(),
            config: config.clone(),
            cache,
            metrics: Arc::new(Metrics::new()),
            l2_chain_id: chain_id,
        },
    );

    let result = http_server_task.await;
    info!("server result : {:?}", result);

    Ok(())
}
