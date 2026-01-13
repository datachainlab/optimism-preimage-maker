use crate::data::finalized_l1_repository::FinalizedL1Repository;
use alloy_primitives::B256;
use axum::async_trait;
use std::time;
use std::time::Duration;
use tokio::fs;
use tokio::fs::DirEntry;

#[derive(Clone)]
pub struct FileFinalizedL1Repository {
    dir: String,
    ttl: Duration,
}
impl FileFinalizedL1Repository {
    pub fn new(parent_dir: &str, ttl: Duration) -> anyhow::Result<Self> {
        let path = std::path::Path::new(parent_dir);
        if !path.exists() {
            return Err(anyhow::anyhow!("directory does not exist: {parent_dir}"));
        }
        if !path.is_dir() {
            return Err(anyhow::anyhow!("path is not a directory: {parent_dir}"));
        }
        Ok(Self {
            dir: parent_dir.to_string(),
            ttl,
        })
    }

    fn path(&self, l1_head_hash: &B256) -> String {
        format!("{}/{}.json", self.dir, l1_head_hash)
    }

    async fn entries(dir: &str) -> anyhow::Result<Vec<DirEntry>> {
        let mut file_list = vec![];
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read dir: {dir:?}, error={e}"))?;
        while let Some(entry) = entries.next_entry().await? {
            match entry.file_name().to_str() {
                None => continue,
                Some(name) => {
                    if !name.ends_with(".tmp") {
                        file_list.push(entry);
                    }
                }
            }
        }
        Ok(file_list)
    }
}

#[async_trait]
impl FinalizedL1Repository for FileFinalizedL1Repository {
    async fn upsert(&self, l1_head_hash: &B256, raw_finalized_l1: String) -> anyhow::Result<()> {
        let path = self.path(l1_head_hash);
        let tmp_path = format!("{path}.tmp");
        fs::write(&tmp_path, raw_finalized_l1).await?;
        fs::rename(&tmp_path, &path).await?;
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
            let modified = metadata.modified()?;
            let expired = modified
                .checked_add(self.ttl)
                .ok_or_else(|| anyhow::anyhow!("expired finalized l1 cache is too new"))?;
            if now >= expired {
                fs::remove_file(entry.path()).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_with_non_existent_dir() {
        let res =
            FileFinalizedL1Repository::new("/path/to/non/existent/dir", Duration::from_secs(1));
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap().to_string(),
            "directory does not exist: /path/to/non/existent/dir"
        );
    }

    #[test]
    fn test_new_with_existing_dir() {
        let temp = std::env::temp_dir();
        let path = temp.to_str().unwrap();
        let res = FileFinalizedL1Repository::new(path, Duration::from_secs(1));
        assert!(res.is_ok());
    }

    fn unique_test_dir(suffix: &str) -> String {
        let mut dir = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        dir.push(format!(
            "optimism_preimage_maker_test_l1_{pid}_{ts}_{suffix}"
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let dir = unique_test_dir("upsert");
        let repo = FileFinalizedL1Repository::new(&dir, Duration::from_secs(1)).expect("new repo");

        let h = B256::from([1u8; 32]);
        let data = "some json data".to_string();

        repo.upsert(&h, data.clone()).await.expect("upsert");

        let got = repo.get(&h).await.expect("get");
        assert_eq!(got, data);

        let missing = B256::from([2u8; 32]);
        let res = repo.get(&missing).await;
        assert!(res.is_err());

        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_purge_expired() {
        let dir = unique_test_dir("purge");
        // Short TTL
        let repo =
            FileFinalizedL1Repository::new(&dir, Duration::from_millis(100)).expect("new repo");

        let h1 = B256::from([0xaau8; 32]);
        let data = "old data".to_string();
        repo.upsert(&h1, data).await.expect("upsert h1");

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(200)).await;

        let h2 = B256::from([0xbbu8; 32]);
        repo.upsert(&h2, "new data".to_string())
            .await
            .expect("upsert h2");

        // Purge should remove h1 but keep h2 (assuming touch updates mod time or we rely on creation time)
        // Note: The implementation uses `metadata.created()`. File creation time is usually fixed.
        // So h1 is created -> wait -> h2 created. h1 should be old enough. h2 is new.
        repo.purge_expired().await.expect("purge");

        assert!(repo.get(&h1).await.is_err(), "h1 should be purged");
        assert!(repo.get(&h2).await.is_ok(), "h2 should be kept");

        tokio::fs::remove_dir_all(dir).await.ok();
    }
    #[tokio::test]
    async fn test_entries_ignores_tmp_files() {
        let dir = unique_test_dir("tmp_ignore");
        let repo = FileFinalizedL1Repository::new(&dir, Duration::from_secs(1)).expect("new repo");

        // Create a normal file via upsert
        let h1 = B256::from([0xccu8; 32]);
        repo.upsert(&h1, "data".to_string()).await.expect("upsert");

        // Manually create a .tmp file
        let path = repo.path(&h1);
        let tmp_path = format!("{path}.tmp");
        tokio::fs::write(&tmp_path, "partial data")
            .await
            .expect("write tmp");

        // Verify entries() ignores the .tmp file
        let entries = FileFinalizedL1Repository::entries(&dir)
            .await
            .expect("entries");
        // Should only have the normal file, not the .tmp file
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].file_name().to_string_lossy().ends_with(".tmp"));

        tokio::fs::remove_dir_all(dir).await.ok();
    }
}
