use alloy_primitives::B256;
use axum::async_trait;

#[async_trait]
pub trait FinalizedL1Repository: Send + Sync {
    async fn upsert(&self, l1_head_hash: &B256, raw_finalized_l1: String) -> anyhow::Result<()>;

    async fn get(&self, l1_head_hash: &B256) -> anyhow::Result<String>;

    async fn purge_expired(&self) -> anyhow::Result<()>;
}
