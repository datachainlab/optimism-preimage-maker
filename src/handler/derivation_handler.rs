use axum::http::StatusCode;
use axum::Json;
use optimism_derivation::derivation::Derivation;
use serde::{Serialize};

#[derive(Serialize)]
pub struct EncodedPreimages {
    pub data: Vec<u8>
}

pub fn derivation(Json(payload) : Json<Derivation>) -> (StatusCode, Json<EncodedPreimages>) {

    //TODO run derivation

    let result = EncodedPreimages {
        data: vec![1, 2, 3]
    };
    (StatusCode::OK, Json(result))
}