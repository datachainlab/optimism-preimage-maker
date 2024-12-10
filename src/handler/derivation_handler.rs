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

pub async fn derivation<T>(
    Json(payload) : Json<Derivation>
) -> (StatusCode, Vec<u8>)
where T: CommsClient + Debug + Send + Sync
{

    (StatusCode::OK, Vec::new())
}