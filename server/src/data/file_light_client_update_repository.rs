use crate::data::light_client_update_repository::LightClientUpdateRepository;
use axum::async_trait;
use std::time;
use std::time::Duration;
use tokio::fs;
use tokio::fs::DirEntry;

#[derive(Clone)]
pub struct FileLightClientUpdateRepository {
    dir: String,
    ttl: Duration,
}

impl FileLightClientUpdateRepository {
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

    fn path(&self, period: u64) -> String {
        format!("{}/lc_update_{}.json", self.dir, period)
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
                    if name.starts_with("lc_update_")
                        && name.ends_with(".json")
                        && !name.ends_with(".tmp")
                    {
                        file_list.push(entry);
                    }
                }
            }
        }
        Ok(file_list)
    }
}

#[async_trait]
impl LightClientUpdateRepository for FileLightClientUpdateRepository {
    async fn upsert(&self, period: u64, raw_light_client_update: String) -> anyhow::Result<()> {
        let path = self.path(period);
        let tmp_path = format!("{path}.tmp");
        fs::write(&tmp_path, raw_light_client_update).await?;
        fs::rename(&tmp_path, &path).await?;
        Ok(())
    }

    async fn get(&self, period: u64) -> anyhow::Result<String> {
        let raw_light_client_update = fs::read_to_string(self.path(period)).await?;
        Ok(raw_light_client_update)
    }

    async fn purge_expired(&self) -> anyhow::Result<()> {
        let now = time::SystemTime::now();
        for entry in Self::entries(&self.dir).await? {
            let metadata = entry.metadata().await?;
            let modified = metadata.modified()?;
            let expired = modified
                .checked_add(self.ttl)
                .ok_or_else(|| anyhow::anyhow!("expired light client update cache is too new"))?;
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

    fn unique_test_dir(suffix: &str) -> String {
        let mut dir = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        dir.push(format!(
            "optimism_preimage_maker_test_lc_{pid}_{ts}_{suffix}"
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn test_new_with_non_existent_dir() {
        let res = FileLightClientUpdateRepository::new(
            "/path/to/non/existent/dir",
            Duration::from_secs(1),
        );
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
        let res = FileLightClientUpdateRepository::new(path, Duration::from_secs(1));
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let dir = unique_test_dir("upsert");
        let repo =
            FileLightClientUpdateRepository::new(&dir, Duration::from_secs(1)).expect("new repo");

        let period = 1664u64;
        let data = r#"{"data": {"finalized_header": {}}}"#.to_string();

        repo.upsert(period, data.clone()).await.expect("upsert");

        let got = repo.get(period).await.expect("get");
        assert_eq!(got, data);

        let missing_period = 9999u64;
        let res = repo.get(missing_period).await;
        assert!(res.is_err());

        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_purge_expired() {
        let dir = unique_test_dir("purge");
        let repo = FileLightClientUpdateRepository::new(&dir, Duration::from_millis(100))
            .expect("new repo");

        let period1 = 100u64;
        let data = "old data".to_string();
        repo.upsert(period1, data).await.expect("upsert period1");

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(200)).await;

        let period2 = 200u64;
        repo.upsert(period2, "new data".to_string())
            .await
            .expect("upsert period2");

        repo.purge_expired().await.expect("purge");

        assert!(repo.get(period1).await.is_err(), "period1 should be purged");
        assert!(repo.get(period2).await.is_ok(), "period2 should be kept");

        tokio::fs::remove_dir_all(dir).await.ok();
    }
}
