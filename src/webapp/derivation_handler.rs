use crate::webapp::DerivationState;
use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use optimism_derivation::derivation::Derivation;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::oneshot;

pub async fn derivation(
    State(state): State<Arc<DerivationState>>,
    Json(payload): Json<Vec<Derivation>>,
) -> (StatusCode, Vec<u8>) {
    let (sender, receiver) = oneshot::channel::<Vec<u8>>();
    let result = state.sender.send((payload, Some(sender))).await;
    match result {
        Ok(_) => match receiver.await {
            Ok(result) => (StatusCode::OK, result),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, vec![]),
        },
        Err(e) => {
            tracing::error!("failed to send derivation request: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, vec![])
        }
    }
}
