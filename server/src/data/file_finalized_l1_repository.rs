use axum::async_trait;
use alloy_primitives::B256;
use tokio::fs;
use crate::data::finalized_l1_repository::FinalizedL1Repository;

#[derive(Clone)]
pub struct FileFinalizedL1Repository {
    dir: String,
}
impl FileFinalizedL1Repository {
    pub fn new(parent_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            dir: parent_dir.to_string(),
        })
    }

    fn path(&self, l1_head_hash: &B256) -> String {
        format!("{}/{}.json", self.dir, l1_head_hash.to_string())
    }
}

#[async_trait]
impl FinalizedL1Repository for FileFinalizedL1Repository {
    async fn upsert(&self, l1_head_hash: &B256, raw_finalized_l1: String) -> anyhow::Result<()> {
        fs::write(self.path(l1_head_hash), raw_finalized_l1).await?;
        Ok(())
    }

    async fn get(&self, l1_head_hash: &B256) -> anyhow::Result<String> {
        let raw_finalized_l1 = fs::read_to_string(self.path(l1_head_hash)).await?;
        Ok(raw_finalized_l1)
    }

}