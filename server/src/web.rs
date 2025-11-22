use crate::derivation::host::single::config::Config;
use crate::derivation::host::single::handler::{Derivation, DerivationConfig, DerivationRequest};
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use alloy_primitives::B256;
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Json;
use kona_genesis::{L1ChainConfig, RollupConfig};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info};

#[derive(Clone)]
pub struct SharedState {
    pub preimage_repository: Arc<dyn PreimageRepository>,
}

async fn start_http_server(addr: &str, state: SharedState) -> Result<()> {
    let app = axum::Router::new()
        .route("/get_preimage", post(get_preimage))
        .route("/get_latest_metadata", post(get_latest_metadata))
        .route("/list_metadata_from", post(list_metadata_from))
        .with_state(Arc::new(state));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn start_http_server_task(addr: &str, state: SharedState) -> JoinHandle<Result<()>> {
    let addr = addr.to_string();
    tokio::spawn(async move {
        start_http_server(&addr, state)
            .await
            .context("http server error")
    })
}

// handler
pub type GetPreimageRequest = PreimageMetadata;

async fn get_preimage(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<GetPreimageRequest>,
) -> (StatusCode, Vec<u8>) {
    info!("derivation request: {:?}", payload);
    if let Err(v) = validate_get_preimage_request(&payload) {
        return (StatusCode::BAD_REQUEST, v.as_bytes().to_vec());
    }

    let result = state.preimage_repository.get(&payload).await;
    match result {
        Ok(preimage) => {
            (StatusCode::OK, preimage)
        }
        Err(e) => {
            info!("failed to get preimage: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, vec![])
        }
    }
}

fn validate_get_preimage_request(payload: &GetPreimageRequest) -> Result<(), &'static str> {
    if payload.l1_head.is_empty() || payload.l1_head.is_zero() {
        error!("invalid l1_head",);
        return Err("invalid l1_head");
    }
    if payload.claimed == 0 {
        error!("invalid l2_block_number",);
        return Err("invalid claimed l2_block_number");
    }
    if payload.agreed >= payload.claimed {
        error!("invalid agreed l2_block_number",);
        return Err("invalid agreed l2_block_number");
    }
    Ok(())
}


async fn get_latest_metadata(
    State(state): State<Arc<SharedState>>,
) -> (StatusCode, Json<Option<PreimageMetadata>>) {
    let result = state.preimage_repository.latest_metadata().await;
    match result {
        Some(metadata) => {
            (StatusCode::OK, Json(Some(metadata)))
        }
        None => {
            info!("failed to get latest metadata",);
            (StatusCode::NOT_FOUND, Json(None))
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListMetadataFromRequest {
   pub gt_claimed: u64,
}

async fn list_metadata_from(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<ListMetadataFromRequest>,
) -> (StatusCode, Json<Vec<PreimageMetadata>>) {

    if payload.gt_claimed == 0 {
        error!("invalid gt_claimed",);
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }

    let result = state.preimage_repository.list_metadata(Some(payload.gt_claimed)).await;
    (StatusCode::OK, Json(result))
}