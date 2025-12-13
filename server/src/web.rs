use crate::data::finalized_l1_repository::FinalizedL1Repository;
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use alloy_primitives::B256;
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{error, info};

#[derive(Clone)]
pub struct SharedState {
    pub preimage_repository: Arc<dyn PreimageRepository>,
    pub finalized_l1_repository: Arc<dyn FinalizedL1Repository>,
}

async fn start_http_server(addr: &str, state: SharedState) -> Result<()> {
    let app = axum::Router::new()
        .route("/get_preimage", post(get_preimage))
        .route("/get_latest_metadata", post(get_latest_metadata))
        .route("/list_metadata", post(list_metadata))
        .route("/get_finalized_l1", post(get_finalized_l1))
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
    info!("request: get_preimage: {:?}", payload);
    if let Err(v) = validate_get_preimage_request(&payload) {
        return (StatusCode::BAD_REQUEST, v.as_bytes().to_vec());
    }

    let result = state.preimage_repository.get(&payload).await;
    match result {
        Ok(preimage) => (StatusCode::OK, preimage),
        Err(e) => {
            error!("failed to get preimage: {:?}", e);
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
    info!("request: get_latest_metadata");
    let result = state.preimage_repository.latest_metadata().await;
    match result {
        Some(metadata) => {
            info!("latest metadata: {:?}", metadata);
            (StatusCode::OK, Json(Some(metadata)))
        }
        None => {
            error!("failed to get latest metadata",);
            (StatusCode::NOT_FOUND, Json(None))
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListMetadataRequest {
    pub lt_claimed: u64,
    pub gt_claimed: u64,
}

async fn list_metadata(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<ListMetadataRequest>,
) -> (StatusCode, Json<Vec<PreimageMetadata>>) {
    info!("request: list_metadata: {:?}", payload);
    if payload.gt_claimed == 0 {
        error!("invalid gt_claimed",);
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }
    if payload.lt_claimed == 0 {
        error!("invalid lt_claimed",);
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }
    if payload.lt_claimed <= payload.gt_claimed {
        error!(
            "invalid lt_claimed {} <= gt_claimed {}",
            payload.lt_claimed, payload.gt_claimed,
        );
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }

    let result = state
        .preimage_repository
        .list_metadata(Some(payload.lt_claimed), Some(payload.gt_claimed))
        .await;
    (StatusCode::OK, Json(result))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetFinalizedL1Request {
    pub l1_head_hash: B256,
}

async fn get_finalized_l1(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<GetFinalizedL1Request>,
) -> (StatusCode, String) {
    info!("request: get_finalized_l1: {:?}", payload);
    let result = state
        .finalized_l1_repository
        .get(&payload.l1_head_hash)
        .await;
    match result {
        Ok(v) => (StatusCode::OK, v),
        Err(e) => {
            error!(
                "failed to get finalized l1: {:?}, hash:{:?}",
                e, payload.l1_head_hash
            );
            (StatusCode::NOT_FOUND, "".to_string())
        }
    }
}
