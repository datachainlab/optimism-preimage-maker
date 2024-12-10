use std::fmt::Debug;
use std::sync::Arc;
use axum::{routing::{get, post}};
use tokio::net::TcpListener;
use anyhow::{Chain, Result};
use axum::http::StatusCode;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use op_alloy_genesis::RollupConfig;
use optimism_derivation::derivation::{Derivation, Derivations};
use crate::oracle::Cache;

mod derivation_handler;
mod oracle;

pub struct DerivationState
{
    pub oracle: Cache,
    pub rollup_config: RollupConfig,
    pub l2_chain_id: u64
}

pub async fn start_http_server(addr: &str , derivation_state: DerivationState) -> Result<()>
{
    let app = axum::Router::new()
        .route("/derivation", post(derivation_handler::derivation))
        .with_state(Arc::new(derivation_state));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}