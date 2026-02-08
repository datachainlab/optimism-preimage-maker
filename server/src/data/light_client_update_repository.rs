use axum::async_trait;

#[async_trait]
pub trait LightClientUpdateRepository: Send + Sync {
    /// Upsert a light client update for the given period.
    async fn upsert(&self, period: u64, raw_light_client_update: String) -> anyhow::Result<()>;

    /// Get a light client update for the given period.
    async fn get(&self, period: u64) -> anyhow::Result<String>;

    /// Purge expired light client updates.
    async fn purge_expired(&self) -> anyhow::Result<()>;
}
