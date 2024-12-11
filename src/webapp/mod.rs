use std::fmt::Debug;
use std::sync::Arc;
use axum::{routing::{get, post}};
use tokio::net::TcpListener;
use anyhow::{Chain, Context, Result};
use axum::http::StatusCode;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use op_alloy_genesis::RollupConfig;
use optimism_derivation::derivation::{Derivation, Derivations};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use crate::derivation::ChannelInterface;

mod derivation_handler;
mod oracle;

pub struct DerivationState
{
    pub sender: mpsc::Sender<ChannelInterface>,
}

async fn start_http_server(addr: &str , derivation_state: DerivationState) -> Result<()>
{
    let app = axum::Router::new()
        .route("/derivation", post(derivation_handler::derivation))
        .with_state(Arc::new(derivation_state));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn start_http_server_task(addr: &str, sender: Sender<ChannelInterface>) -> JoinHandle<Result<()>> {
    let addr = addr.to_string();
    tokio::spawn(async move {
        let derivation_state = DerivationState {
            sender,
        };
        start_http_server(&addr, derivation_state).await.context("http server error")
    })
}