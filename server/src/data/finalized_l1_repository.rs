use alloy_primitives::B256;
use axum::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Data stored for each finalized L1 head hash.
/// Contains the finality update and the light client update for the corresponding period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedL1Data {
    /// Raw JSON of the light client finality update from beacon API.
    pub raw_finality_update: Value,
    /// Raw JSON of the light client update for the signature_slot's period.
    /// This ensures consistency with the relayer's period calculation.
    pub raw_light_client_update: Value,
    /// The period of the light client update (based on signature_slot).
    pub period: u64,
}

#[async_trait]
pub trait FinalizedL1Repository: Send + Sync {
    async fn upsert(&self, l1_head_hash: &B256, data: FinalizedL1Data) -> anyhow::Result<()>;

    async fn get(&self, l1_head_hash: &B256) -> anyhow::Result<FinalizedL1Data>;

    async fn purge_expired(&self) -> anyhow::Result<()>;
}
