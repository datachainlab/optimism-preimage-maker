mod fetcher;
mod util;
mod host;
mod server;
mod preimage;
mod config;

use anyhow::{Result};
use clap::Parser;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use host::start_server_and_native_client;
use config::Config;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()>{
    let filter = filter::EnvFilter::from_default_env().add_directive("optimism_preimage_host=info".parse()?);
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    let config = Config::parse();

    start_server_and_native_client(config).await?;
    Ok(())
}
