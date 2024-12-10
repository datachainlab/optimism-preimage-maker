use std::fmt::Debug;
use std::sync::Arc;
use anyhow::Context;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use kona_preimage::{CommsClient, HintWriterClient, PreimageOracleClient};
use optimism_derivation::derivation::Derivation;
use serde::{Serialize};
use crate::handler::DerivationState;
use crate::handler::oracle::{PreimageIO, PreimageTraceable};

pub async fn derivation<T>(
    State(state): State<DerivationState<T>>,
    Json(payload) : Json<Derivation>
) -> (StatusCode, Vec<u8>)
where T: CommsClient + Debug + Send + Sync
{

    // For the sake of collect used preimages.
    let oracle = Arc::new(PreimageIO::new(state.oracle.clone()));

    tracing::info!("start derivation claiming number = {}, request={:?}", payload.l2_block_number, payload);
    let result = payload.verify(state.l2_chain_id, &state.rollup_config, oracle.clone()).await;
    match result {
        Ok(_) => {
            tracing::info!("end derivation claiming number = {}", payload.l2_block_number);
            let used_preimages = oracle.preimages();
            tracing::info!("used preimage size : {}", used_preimages.len());
            (StatusCode::OK, used_preimages)
        },
        Err(e) => {
            tracing::error!("end derivation claiming number = {} with error = {:?}", payload.l2_block_number, e);
            (StatusCode::INTERNAL_SERVER_ERROR, vec![])
        }
    }
}