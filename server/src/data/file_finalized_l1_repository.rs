use std::time;
use std::time::Duration;
use axum::async_trait;
use alloy_primitives::B256;
use tokio::fs;
use tokio::fs::DirEntry;
use crate::data::finalized_l1_repository::FinalizedL1Repository;

#[derive(Clone)]
pub struct FileFinalizedL1Repository {
    dir: String,
    ttl: Duration
}
impl FileFinalizedL1Repository {
    pub fn new(parent_dir: &str, ttl: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            dir: parent_dir.to_string(),
            ttl
        })
    }

    fn path(&self, l1_head_hash: &B256) -> String {
        format!("{}/{}.json", self.dir, l1_head_hash.to_string())
    }

    async fn entries(dir: &str) -> anyhow::Result<Vec<DirEntry>> {
        let mut file_list = vec![];
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read dir: {dir:?}, error={e}"))?;
        while let Some(entry) = entries.next_entry().await? {
            match entry.file_name().to_str() {
                None => continue,
                Some(_) => {
                    file_list.push(entry);
                }
            }
        }
        Ok(file_list)
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

    async fn purge_expired(&self) -> anyhow::Result<()> {
        let now = time::SystemTime::now();
        for entry in Self::entries(&self.dir).await? {
            let metadata = entry.metadata().await?;
            let created = metadata.created()?;
            let expired = created.checked_add(self.ttl).ok_or_else(|| anyhow::anyhow!("expired finalized l1 cache is too new"))?;
            if now >= expired {
                fs::remove_file(entry.path()).await?;
            }
        }
        Ok(())
    }
}