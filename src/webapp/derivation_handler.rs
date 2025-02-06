use crate::webapp::DerivationState;
use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use log::info;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use alloy_primitives::B256;
use kona_host::HostOrchestrator;
use tokio::sync::oneshot;
use crate::host::single::orchestrator::DerivationRequest;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub l1_head_hash: B256,
    pub agreed_l2_head_hash: B256,
    pub agreed_l2_output_root: B256,
    pub l2_output_root: B256,
    pub l2_block_number: u64,
}

pub async fn derivation(
    State(state): State<Arc<DerivationState>>,
    Json(payload): Json<Request>,
) -> (StatusCode, Vec<u8>) {
    info!("derivation request: {:?}", payload);
    let derivation = DerivationRequest {
        config: state.config.clone(),
        rollup_config: state.rollup_config.clone(),
        l2_chain_id: state.l2_chain_id,
        agreed_l2_head_hash: payload.agreed_l2_head_hash,
        agreed_l2_output_root: payload.agreed_l2_output_root,
        l1_head_hash: payload.l1_head_hash,
        l2_output_root: payload.l2_output_root,
        l2_block_number: payload.l2_block_number,
    };
    match derivation.start().await {
        Ok(_) => {
            info!("derivation success");
            (StatusCode::OK, vec![])
        },
        Err(e) => {
            info!("failed to run derivation: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, vec![])
        }
    }
}
