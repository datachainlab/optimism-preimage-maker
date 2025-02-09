use anyhow::{Chain, Context, Result};
use axum::http::StatusCode;
use axum::routing::{get, post};
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use std::fmt::Debug;
use std::sync::Arc;
use maili_genesis::RollupConfig;
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use crate::host::single::cli::SingleChainHostCli;

mod derivation_handler;
pub mod oracle;

pub struct DerivationState {
    pub rollup_config: RollupConfig,
    pub config: SingleChainHostCli,
    pub l2_chain_id: u64
}

async fn start_http_server(addr: &str, derivation_state: DerivationState) -> Result<()> {
    let app = axum::Router::new()
        .route("/derivation", post(derivation_handler::derivation))
        .with_state(Arc::new(derivation_state));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn start_http_server_task(
    addr: &str,
    state: DerivationState,
) -> JoinHandle<Result<()>> {
    let addr = addr.to_string();
    tokio::spawn(async move {
        start_http_server(&addr, state)
            .await
            .context("http server error")
    })
}
