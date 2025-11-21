use std::sync::{Arc, RwLock};
use axum::async_trait;
use tokio::fs;
use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};

pub struct FilePreimageRepository {
    dir: String,
    metadata_list: Arc<RwLock<Vec<PreimageMetadata>>>
}

impl FilePreimageRepository {
    pub async fn new(parent_dir: &str) -> anyhow::Result<Self> {
        let metadata_list = Self::load_metadata(&parent_dir);
        Ok(Self {
            dir: parent_dir.to_string(),
            metadata_list: Arc::new(RwLock::new(metadata_list.await?))
        })
    }

    async fn load_metadata(dir: &str) -> anyhow::Result<Vec<PreimageMetadata>> {

        let mut metadata_list = vec![];

        let mut entries = fs::read_dir(dir).await.map_err(|e | {
            anyhow::anyhow!("failed to read dir: {:?}, error={}", dir, e)
        })?;
        while let Some(entry) = entries.next_entry().await? {
            match entry.file_name().to_str() {
                None => continue,
                Some(name) => {
                    let metadata = PreimageMetadata::try_from(name);
                    match metadata {
                        Err(_) => continue,
                        Ok(_) => metadata_list.push(metadata.unwrap())
                    }
                }
            }
        }
        Ok(metadata_list)
    }

    fn path(&self,metadata: &PreimageMetadata) -> String {
        format!("{}/{}_{}_{}.bin", self.dir, metadata.claimed, metadata.agreed, metadata.l1_head.to_string())
    }
}

#[async_trait]
impl PreimageRepository for FilePreimageRepository {
    async fn upsert(&self, metadata: PreimageMetadata, preimage: Vec<u8>) -> anyhow::Result<()> {
        let path = self.path(&metadata);
        fs::write(&path, preimage).await?;
        // NOTE: Process should be restarted when writing metadata is failed to avoid dirty metadata.
        let mut lock = self.metadata_list.write().unwrap();;
        lock.push(metadata);
        Ok(())
    }
    async fn get(&self, metadata: PreimageMetadata) -> anyhow::Result<Vec<u8>> {
        let path =  self.path(&metadata);
        let preimage = fs::read(&path).await?;
        Ok(preimage)
    }
    async fn list_metadata(&self, gt_claimed: Option<u64>) -> anyhow::Result<Vec<PreimageMetadata>> {
        let read_only_metadata = {
            let lock = self.metadata_list.read().unwrap();
            lock.clone()
        };
        match gt_claimed {
            None => Ok(read_only_metadata),
            Some(gt_claimed) => Ok(read_only_metadata.into_iter().filter(|m| m.claimed > gt_claimed).collect())
        }
    }

    async fn latest_metadata(&self) -> Option<PreimageMetadata> {
        let lock = self.metadata_list.read().unwrap();
        let last = lock.last().cloned();
        last
    }
}