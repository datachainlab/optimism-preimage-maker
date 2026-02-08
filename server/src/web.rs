use crate::data::finalized_l1_repository::FinalizedL1Repository;
use crate::data::light_client_update_repository::LightClientUpdateRepository;
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
    pub light_client_update_repository: Arc<dyn LightClientUpdateRepository>,
}

async fn start_http_server(addr: &str, state: SharedState) -> Result<()> {
    let app = axum::Router::new()
        .route("/get_preimage", post(get_preimage))
        .route("/get_latest_metadata", post(get_latest_metadata))
        .route("/list_metadata", post(list_metadata))
        .route("/get_finalized_l1", post(get_finalized_l1))
        .route("/get_light_client_update", post(get_light_client_update))
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
            "invalid range: lt_claimed ({}) must be greater than gt_claimed ({})",
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetLightClientUpdateRequest {
    pub period: u64,
}

async fn get_light_client_update(
    State(state): State<Arc<SharedState>>,
    Json(payload): Json<GetLightClientUpdateRequest>,
) -> (StatusCode, String) {
    info!("request: get_light_client_update: {:?}", payload);
    let result = state
        .light_client_update_repository
        .get(payload.period)
        .await;
    match result {
        Ok(v) => (StatusCode::OK, v),
        Err(e) => {
            error!(
                "failed to get light client update: {:?}, period:{}",
                e, payload.period
            );
            (StatusCode::NOT_FOUND, "".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::async_trait;
    use std::sync::Mutex;

    struct MockPreimageRepository {
        data: Arc<Mutex<Vec<PreimageMetadata>>>,
        should_fail: bool,
    }

    #[async_trait]
    impl PreimageRepository for MockPreimageRepository {
        async fn upsert(
            &self,
            _metadata: PreimageMetadata,
            _preimage: Vec<u8>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn get(&self, metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>> {
            if self.should_fail {
                return Err(anyhow::anyhow!("mock error"));
            }
            if metadata.claimed == 999 {
                return Ok(vec![1, 2, 3]);
            }
            Ok(vec![])
        }
        async fn list_metadata(&self, lt: Option<u64>, gt: Option<u64>) -> Vec<PreimageMetadata> {
            let data = self.data.lock().unwrap();
            data.iter()
                .filter(|m| {
                    if let Some(l) = lt {
                        if m.claimed >= l {
                            return false;
                        }
                    }
                    if let Some(g) = gt {
                        if m.claimed <= g {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect()
        }
        async fn latest_metadata(&self) -> Option<PreimageMetadata> {
            if self.should_fail {
                return None;
            }
            self.data.lock().unwrap().last().cloned()
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockFinalizedL1Repository {
        data: Arc<Mutex<std::collections::HashMap<B256, String>>>,
    }

    #[async_trait]
    impl FinalizedL1Repository for MockFinalizedL1Repository {
        async fn upsert(
            &self,
            l1_head_hash: &B256,
            raw_finalized_l1: String,
        ) -> anyhow::Result<()> {
            self.data
                .lock()
                .unwrap()
                .insert(*l1_head_hash, raw_finalized_l1);
            Ok(())
        }
        async fn get(&self, l1_head_hash: &B256) -> anyhow::Result<String> {
            self.data
                .lock()
                .unwrap()
                .get(l1_head_hash)
                .cloned()
                .ok_or(anyhow::anyhow!("not found"))
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockLightClientUpdateRepository {
        data: Arc<Mutex<std::collections::HashMap<u64, String>>>,
    }

    #[async_trait]
    impl LightClientUpdateRepository for MockLightClientUpdateRepository {
        async fn upsert(&self, period: u64, raw: String) -> anyhow::Result<()> {
            self.data.lock().unwrap().insert(period, raw);
            Ok(())
        }
        async fn get(&self, period: u64) -> anyhow::Result<String> {
            self.data
                .lock()
                .unwrap()
                .get(&period)
                .cloned()
                .ok_or(anyhow::anyhow!("not found"))
        }
        async fn purge_expired(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn setup_state() -> Arc<SharedState> {
        let repo = MockPreimageRepository {
            data: Arc::new(Mutex::new(vec![
                PreimageMetadata {
                    l1_head: B256::repeat_byte(1),
                    claimed: 100,
                    agreed: 90,
                },
                PreimageMetadata {
                    l1_head: B256::repeat_byte(2),
                    claimed: 200,
                    agreed: 190,
                },
            ])),
            should_fail: false,
        };
        let l1_repo = MockFinalizedL1Repository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let lc_repo = MockLightClientUpdateRepository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        Arc::new(SharedState {
            preimage_repository: Arc::new(repo),
            finalized_l1_repository: Arc::new(l1_repo),
            light_client_update_repository: Arc::new(lc_repo),
        })
    }

    #[tokio::test]
    async fn test_get_preimage_validation() {
        let state = setup_state();

        // Invalid l1_head
        let req = GetPreimageRequest {
            l1_head: B256::ZERO,
            claimed: 100,
            agreed: 90,
        };
        let (status, _) = get_preimage(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // Invalid claimed
        let req = GetPreimageRequest {
            l1_head: B256::repeat_byte(1),
            claimed: 0,
            agreed: 90,
        };
        let (status, _) = get_preimage(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // Invalid agreed >= claimed
        let req = GetPreimageRequest {
            l1_head: B256::repeat_byte(1),
            claimed: 100,
            agreed: 100,
        };
        let (status, _) = get_preimage(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_preimage_success() {
        let state = setup_state();
        let req = GetPreimageRequest {
            l1_head: B256::repeat_byte(1),
            claimed: 999, // Trigger mock success with data
            agreed: 900,
        };
        let (status, data) = get_preimage(State(state), Json(req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(data, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_get_preimage_error() {
        let repo = MockPreimageRepository {
            data: Arc::new(Mutex::new(vec![])),
            should_fail: true,
        };
        let l1_repo = MockFinalizedL1Repository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let lc_repo = MockLightClientUpdateRepository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let state = Arc::new(SharedState {
            preimage_repository: Arc::new(repo),
            finalized_l1_repository: Arc::new(l1_repo),
            light_client_update_repository: Arc::new(lc_repo),
        });

        let req = GetPreimageRequest {
            l1_head: B256::repeat_byte(1),
            claimed: 100,
            agreed: 90,
        };
        let (status, _) = get_preimage(State(state), Json(req)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_get_latest_metadata() {
        let state = setup_state();
        let (status, Json(opt)) = get_latest_metadata(State(state)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().claimed, 200);
    }

    #[tokio::test]
    async fn test_get_latest_metadata_empty() {
        let repo = MockPreimageRepository {
            data: Arc::new(Mutex::new(vec![])),
            should_fail: true, // Mock behavior for None
        };
        let l1_repo = MockFinalizedL1Repository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let lc_repo = MockLightClientUpdateRepository {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let state = Arc::new(SharedState {
            preimage_repository: Arc::new(repo),
            finalized_l1_repository: Arc::new(l1_repo),
            light_client_update_repository: Arc::new(lc_repo),
        });

        let (status, Json(opt)) = get_latest_metadata(State(state)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(opt.is_none());
    }

    #[tokio::test]
    async fn test_list_metadata_validation() {
        let state = setup_state();

        // lt_claimed <= gt_claimed
        let req = ListMetadataRequest {
            lt_claimed: 100,
            gt_claimed: 100,
        };
        let (status, _) = list_metadata(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // zero gt_claimed
        let req = ListMetadataRequest {
            lt_claimed: 100,
            gt_claimed: 0,
        };
        let (status, _) = list_metadata(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_metadata_success() {
        let state = setup_state();
        let req = ListMetadataRequest {
            lt_claimed: 210,
            gt_claimed: 90,
        };
        let (status, Json(vec)) = list_metadata(State(state), Json(req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(vec.len(), 2);
    }

    #[tokio::test]
    async fn test_get_finalized_l1() {
        let state = setup_state();
        let hash = B256::repeat_byte(0x99);
        state
            .finalized_l1_repository
            .upsert(&hash, "data".into())
            .await
            .unwrap();

        let req = GetFinalizedL1Request { l1_head_hash: hash };
        let (status, data) = get_finalized_l1(State(state), Json(req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(data, "data");

        let req_fail = GetFinalizedL1Request {
            l1_head_hash: B256::ZERO,
        };
        let (status, _) = get_finalized_l1(State(setup_state()), Json(req_fail)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_light_client_update() {
        let state = setup_state();
        let period = 1664u64;
        state
            .light_client_update_repository
            .upsert(period, r#"{"data":{}}"#.into())
            .await
            .unwrap();

        let req = GetLightClientUpdateRequest { period };
        let (status, data) = get_light_client_update(State(state), Json(req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(data, r#"{"data":{}}"#);

        let req_fail = GetLightClientUpdateRequest { period: 9999 };
        let (status, _) = get_light_client_update(State(setup_state()), Json(req_fail)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
