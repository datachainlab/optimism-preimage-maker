use std::fmt::Debug;
use std::sync::Arc;
use axum::{routing::{get, post}};
use tokio::net::TcpListener;
use anyhow::{Chain, Result};
use axum::http::StatusCode;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use op_alloy_genesis::RollupConfig;
use optimism_derivation::derivation::{Derivation, Derivations};

mod derivation_handler;
mod oracle;

pub struct DerivationState<T>
where T: CommsClient + Debug + Send + Sync
{
    pub oracle: T,
    pub rollup_config: RollupConfig,
    pub l2_chain_id: u64
}

async fn start_server<T>(addr: &str , derivation_state: DerivationState<T>) -> Result<()>
where T: CommsClient + Debug + Send + Sync + 'static
{
    let app = axum::Router::new()
        .route("/derivation", post(derivation_handler::derivation::<T>))
        .with_state(Arc::new(derivation_state));

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}